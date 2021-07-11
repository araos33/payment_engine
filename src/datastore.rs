use crate::error::{PaymentEngineError, PaymentEngineResult};
use crate::model::{Account, Transaction};
use lru::LruCache;
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};
use std::collections::HashMap;
use std::time::Duration;

const TRANSACTION_DB_PATH: &str = "pe_transaction.db";
const FLUSH_INTERVAL_MICROSECONDS: u64 = 500;
const CACHE_SIZE: usize = 50_000;

pub trait DatastoreOperations {
    fn retrieve_transaction(
        &mut self,
        transaction_id: u32,
    ) -> PaymentEngineResult<Option<Transaction>>;
    fn save_transaction(&mut self, transaction: Transaction) -> PaymentEngineResult<()>;
    fn retrieve_account(&self, client_id: u16) -> PaymentEngineResult<Option<Account>>;
    fn save_account(&mut self, account: Account) -> PaymentEngineResult<()>;
    fn retrieve_all_accounts(&self) -> PaymentEngineResult<Vec<Account>>;
    fn set_transaction_disputed(
        &mut self,
        transaction_id: u32,
        disputed: bool,
    ) -> PaymentEngineResult<()>;
    fn remove_transaction_from_cache(&mut self, transaction_id: u32) -> PaymentEngineResult<()>;
}

pub struct PickleDatastore {
    transaction_db: PickleDb,
    accounts: HashMap<u16, Account>,
    disputed_transactions_cache: LruCache<u32, Transaction>,
}

impl PickleDatastore {
    pub fn new() -> Self {
        let transaction_db = PickleDb::new(
            TRANSACTION_DB_PATH,
            PickleDbDumpPolicy::PeriodicDump(Duration::from_micros(FLUSH_INTERVAL_MICROSECONDS)),
            SerializationMethod::Bin,
        );

        PickleDatastore {
            transaction_db,
            accounts: HashMap::default(),
            disputed_transactions_cache: LruCache::new(CACHE_SIZE),
        }
    }
}

impl DatastoreOperations for PickleDatastore {
    fn retrieve_transaction(
        &mut self,
        transaction_id: u32,
    ) -> PaymentEngineResult<Option<Transaction>> {
        match self.disputed_transactions_cache.get(&transaction_id) {
            Some(transaction) => Ok(Option::from(transaction.clone())),
            None => match self
                .transaction_db
                .get::<String>(&*transaction_id.to_string())
            {
                Some(json) => {
                    let transaction: Transaction = serde_json::from_str(&*json)?;

                    Ok(Option::from(transaction))
                }
                None => Ok(None),
            },
        }
    }

    fn save_transaction(&mut self, transaction: Transaction) -> PaymentEngineResult<()> {
        let json = serde_json::to_string(&transaction)?;

        self.transaction_db
            .set(&*transaction.transaction_id.to_string(), &json)?;

        Ok(())
    }

    fn retrieve_account(&self, client_id: u16) -> PaymentEngineResult<Option<Account>> {
        Ok(self.accounts.get(&client_id).cloned())
    }

    fn save_account(&mut self, account: Account) -> PaymentEngineResult<()> {
        self.accounts.insert(account.client_id, account);

        Ok(())
    }

    fn retrieve_all_accounts(&self) -> PaymentEngineResult<Vec<Account>> {
        Ok(self.accounts.values().cloned().collect())
    }

    fn set_transaction_disputed(
        &mut self,
        transaction_id: u32,
        disputed: bool,
    ) -> PaymentEngineResult<()> {
        match self.retrieve_transaction(transaction_id)? {
            Some(mut transaction) => {
                transaction.disputed = disputed;

                self.disputed_transactions_cache
                    .put(transaction_id, transaction.clone());
                self.save_transaction(transaction)?;
            }
            None => return Err(PaymentEngineError::DisputedValueChange),
        };

        Ok(())
    }

    fn remove_transaction_from_cache(&mut self, transaction_id: u32) -> PaymentEngineResult<()> {
        self.disputed_transactions_cache.pop(&transaction_id);

        Ok(())
    }
}
