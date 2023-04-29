use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::cpi;
use crate::{
    error::NautilusError, Create, Mut, NautilusAccountInfo, NautilusCreate, NautilusData,
    NautilusMut, NautilusRecord, NautilusSigner, NautilusTransferLamports, Signer, Wallet,
};

/// The account inner data for the `NautilusIndex`.
///
/// This `index` is simply a Hash Map that stores the current record count for each table, where
/// the `String` key is the table name and the `u32` value is the current count.
///
/// This data is kept in one single account and used as a reference to enable autoincrementing of records.
#[derive(borsh::BorshDeserialize, borsh::BorshSerialize, Clone, Default)]
pub struct NautilusIndexData {
    pub index: std::collections::HashMap<String, u32>,
}

impl NautilusIndexData {
    /// Get the current record count for a table.
    pub fn get_count(&self, table_name: &str) -> Option<u32> {
        self.index.get(&(table_name.to_string())).copied()
    }

    /// Get the next record count for a table.
    pub fn get_next_count(&self, table_name: &str) -> u32 {
        match self.index.get(&(table_name.to_string())) {
            Some(count) => count + 1,
            None => 1,
        }
    }

    /// Add a new record to the index.
    pub fn add_record(&mut self, table_name: &str) -> u32 {
        match self.index.get_mut(&(table_name.to_string())) {
            Some(count) => {
                *count += 1;
                *count
            }
            None => {
                self.index.insert(table_name.to_string(), 1);
                1
            }
        }
    }
}

impl NautilusData for NautilusIndexData {
    const TABLE_NAME: &'static str = "nautilus_index";

    const AUTO_INCREMENT: bool = false;

    fn primary_key(&self) -> Vec<u8> {
        vec![0]
    }

    fn check_authorities(&self, _accounts: Vec<AccountInfo>) -> Result<(), ProgramError> {
        Ok(())
    }

    fn count_authorities(&self) -> u8 {
        0
    }
}

/// The special Nautilus object representing the accompanying index for a Nautilus program.
///
/// The underlying account - designated in field `account_info` - is the Nautilus Index.
///
/// This single account is used as a reference to enable autoincrementing of records.
#[derive(borsh::BorshDeserialize, borsh::BorshSerialize, Clone)]
pub struct NautilusIndex<'a> {
    pub program_id: &'a Pubkey,
    pub account_info: Box<AccountInfo<'a>>,
    pub data: NautilusIndexData,
}

impl<'a> NautilusIndex<'a> {
    /// Instantiate a new `NautilusIndex` without loading the account inner data from on-chain.
    pub fn new(program_id: &'a Pubkey, account_info: Box<AccountInfo<'a>>) -> Self {
        Self {
            program_id,
            account_info,
            data: NautilusIndexData::default(),
        }
    }

    /// Instantiate a new `NautilusIndex` and load the account inner data from on-chain.
    pub fn load(
        program_id: &'a Pubkey,
        account_info: Box<AccountInfo<'a>>,
    ) -> Result<Self, ProgramError> {
        let data = match NautilusIndexData::try_from_slice(match &account_info.try_borrow_data() {
            Ok(acct_data) => acct_data,
            Err(_) => {
                return Err(NautilusError::LoadDataFailed(
                    NautilusIndexData::TABLE_NAME.to_string(),
                    account_info.key.to_string(),
                )
                .into())
            }
        }) {
            Ok(state_data) => state_data,
            Err(_) => {
                return Err(NautilusError::DeserializeDataFailed(
                    NautilusIndexData::TABLE_NAME.to_string(),
                    account_info.key.to_string(),
                )
                .into())
            }
        };
        Ok(Self {
            program_id,
            account_info,
            data,
        })
    }

    pub fn get_count(&self, table_name: &str) -> Option<u32> {
        self.data.get_count(table_name)
    }

    pub fn get_next_count(&self, table_name: &str) -> u32 {
        self.data.get_next_count(table_name)
    }

    pub fn add_record(
        &mut self,
        table_name: &str,
        fee_payer: impl NautilusSigner<'a>,
    ) -> Result<u32, ProgramError> {
        let count = self.data.add_record(table_name);
        cpi::system::transfer(
            fee_payer,
            Mut::<Self>::new(self.clone()),
            self.required_rent()? - self.lamports(),
        )?;
        self.account_info.realloc(self.span()?, true)?;
        self.data
            .serialize(&mut &mut self.account_info.data.borrow_mut()[..])?;
        Ok(count)
    }
}

impl<'a> NautilusAccountInfo<'a> for NautilusIndex<'a> {
    fn account_info(&self) -> Box<AccountInfo<'a>> {
        self.account_info.clone()
    }

    fn key(&self) -> &'a Pubkey {
        self.account_info.key
    }

    fn is_signer(&self) -> bool {
        self.account_info.is_signer
    }

    fn is_writable(&self) -> bool {
        self.account_info.is_writable
    }

    fn lamports(&self) -> u64 {
        self.account_info.lamports()
    }

    fn mut_lamports(&self) -> Result<std::cell::RefMut<'_, &'a mut u64>, ProgramError> {
        self.account_info.try_borrow_mut_lamports()
    }

    fn owner(&self) -> &'a Pubkey {
        self.account_info.owner
    }

    fn span(&self) -> Result<usize, ProgramError> {
        Ok(self.data.try_to_vec()?.len())
    }
}

impl<'a> NautilusRecord<'a> for NautilusIndex<'a> {
    fn primary_key(&self) -> Vec<u8> {
        self.data.primary_key()
    }

    fn seeds(&self) -> [Vec<u8>; 2] {
        self.data.seeds()
    }

    fn pda(&self) -> (Pubkey, u8) {
        self.data.pda(self.program_id)
    }

    fn check_authorities(&self, accounts: Vec<AccountInfo>) -> Result<(), ProgramError> {
        self.data.check_authorities(accounts)
    }

    fn count_authorities(&self) -> u8 {
        self.data.count_authorities()
    }
}

impl<'a> NautilusTransferLamports<'a> for NautilusIndex<'a> {
    fn transfer_lamports(self, to: impl NautilusMut<'a>, amount: u64) -> ProgramResult {
        let from = self.account_info;
        **from.try_borrow_mut_lamports()? -= amount;
        **to.mut_lamports()? += amount;
        Ok(())
    }
}

impl<'a> NautilusCreate<'a> for Create<'a, NautilusIndex<'a>> {
    fn create(&mut self) -> ProgramResult {
        let payer = Signer::new(Wallet {
            account_info: self.fee_payer.clone(),
            system_program: self.system_program.clone(),
        });
        let data = NautilusIndexData {
            index: std::collections::HashMap::new(),
        };
        let data_pointer = Box::new(data);
        cpi::system::create_record(
            self.self_account.clone(),
            self.self_account.program_id,
            payer,
            self.system_program.to_owned(),
            data_pointer.clone(),
        )?;
        self.self_account.data = *data_pointer;
        Ok(())
    }

    fn create_with_payer(&mut self, payer: impl NautilusSigner<'a>) -> ProgramResult {
        let data = NautilusIndexData {
            index: std::collections::HashMap::new(),
        };
        let data_pointer = Box::new(data);
        cpi::system::create_record(
            self.self_account.clone(),
            self.self_account.program_id,
            payer,
            self.system_program.to_owned(),
            data_pointer.clone(),
        )?;
        self.self_account.data = *data_pointer;
        Ok(())
    }
}
