use crate::datastore::DatastoreOperations;
use crate::error::{PaymentEngineError, PaymentEngineResult};
use crate::model::{Account, Transaction, TransactionType};
use csv::{ReaderBuilder, Trim, WriterBuilder};

pub struct PaymentService {
    datastore: Box<dyn DatastoreOperations>,
}

impl PaymentService {
    pub fn new(datastore: Box<dyn DatastoreOperations>) -> Box<Self> {
        Box::new(PaymentService { datastore })
    }

    pub fn run(&mut self, csv_path: &str) -> PaymentEngineResult<()> {
        let mut reader = ReaderBuilder::new()
            .has_headers(true)
            .trim(Trim::All)
            .from_path(csv_path)?;

        for entry in reader.deserialize() {
            let transaction: Transaction = match entry {
                Ok(transaction) => transaction,
                Err(e) => {
                    warn!(
                        "Invalid data, cannot deserialize row to transaction Error: {}",
                        e
                    );
                    continue;
                }
            };
            let mut account = self.retrieve_account(transaction.client_id)?;

            match self.process_transaction(&transaction, &mut account) {
                Ok(_) => {}
                Err(e) => {
                    warn!("{} | {:?} {:?}", e, account, transaction)
                }
            };
        }

        self.write_accounts()?;

        Ok(())
    }

    fn process_transaction(
        &mut self,
        transaction: &Transaction,
        account: &mut Account,
    ) -> PaymentEngineResult<()> {
        match transaction.r#type {
            TransactionType::Deposit => self.handle_deposit(transaction, account),
            TransactionType::Withdrawal => self.handle_withdrawal(transaction, account),
            TransactionType::Dispute => self.handle_dispute(transaction, account),
            TransactionType::Resolve => self.handle_resolve(transaction, account),
            TransactionType::Chargeback => self.handle_chargeback(transaction, account),
        }
    }

    fn handle_deposit(
        &mut self,
        transaction: &Transaction,
        account: &mut Account,
    ) -> PaymentEngineResult<()> {
        let amount = match transaction.amount {
            Some(amount) => amount,
            None => return Err(PaymentEngineError::NoAmount),
        };
        account.available += amount;
        account.total += amount;

        self.datastore.save_transaction(transaction.clone())?;
        self.save_account_to_datastore(account)?;

        Ok(())
    }

    fn handle_withdrawal(
        &mut self,
        transaction: &Transaction,
        account: &mut Account,
    ) -> PaymentEngineResult<()> {
        let amount = match transaction.amount {
            Some(amount) => {
                if amount > account.available {
                    return Err(PaymentEngineError::InsufficientAccountFunds);
                } else {
                    amount
                }
            }
            None => return Err(PaymentEngineError::NoAmount),
        };
        account.available -= amount;
        account.total -= amount;

        self.datastore.save_transaction(transaction.clone())?;
        self.save_account_to_datastore(account)?;

        Ok(())
    }

    fn handle_dispute(
        &mut self,
        transaction: &Transaction,
        account: &mut Account,
    ) -> PaymentEngineResult<()> {
        let referenced_transaction = self.retrieve_transaction(transaction.transaction_id)?;
        let referenced_transaction_id = referenced_transaction.transaction_id;

        if referenced_transaction.disputed {
            return Err(PaymentEngineError::TransactionAlreadyDisputed);
        }

        let amount = match referenced_transaction.amount {
            Some(amount) => amount,
            None => return Err(PaymentEngineError::NoAmount),
        };

        match referenced_transaction.r#type {
            TransactionType::Deposit => {
                account.available -= amount;
                account.held += amount;
            }
            TransactionType::Withdrawal => {
                account.held += amount;
                account.total += amount;
            }
            _ => return Err(PaymentEngineError::InvalidDisputedTransactionType),
        }

        self.datastore
            .set_transaction_disputed(referenced_transaction_id, true)?;
        self.save_account_to_datastore(account)?;

        Ok(())
    }

    fn handle_resolve(
        &mut self,
        transaction: &Transaction,
        account: &mut Account,
    ) -> PaymentEngineResult<()> {
        let referenced_transaction = self.retrieve_transaction(transaction.transaction_id)?;
        let referenced_transaction_id = referenced_transaction.transaction_id;

        if !referenced_transaction.disputed {
            return Err(PaymentEngineError::TransactionNotDisputed);
        }

        let amount = match referenced_transaction.amount {
            Some(amount) => amount,
            None => return Err(PaymentEngineError::NoAmount),
        };

        match referenced_transaction.r#type {
            TransactionType::Deposit | TransactionType::Withdrawal => {
                account.available += amount;
                account.held -= amount;
            }
            _ => return Err(PaymentEngineError::InvalidDisputedTransactionType),
        }

        self.remove_disputed_state(referenced_transaction_id)?;
        self.save_account_to_datastore(account)?;

        Ok(())
    }

    fn handle_chargeback(
        &mut self,
        transaction: &Transaction,
        account: &mut Account,
    ) -> PaymentEngineResult<()> {
        let referenced_transaction = self.retrieve_transaction(transaction.transaction_id)?;
        let referenced_transaction_id = referenced_transaction.transaction_id;

        if !referenced_transaction.disputed {
            return Err(PaymentEngineError::TransactionNotDisputed);
        }

        let amount = match referenced_transaction.amount {
            Some(amount) => amount,
            None => return Err(PaymentEngineError::NoAmount),
        };

        match referenced_transaction.r#type {
            TransactionType::Deposit | TransactionType::Withdrawal => {
                account.held -= amount;
                account.total -= amount;
                account.locked = true;
            }
            _ => return Err(PaymentEngineError::InvalidDisputedTransactionType),
        }

        self.remove_disputed_state(referenced_transaction_id)?;
        self.save_account_to_datastore(account)?;

        Ok(())
    }

    fn remove_disputed_state(&mut self, referenced_transaction_id: u32) -> PaymentEngineResult<()> {
        self.datastore
            .set_transaction_disputed(referenced_transaction_id, false)?;
        self.datastore
            .remove_transaction_from_cache(referenced_transaction_id)?;

        Ok(())
    }

    fn retrieve_transaction(&mut self, transaction_id: u32) -> PaymentEngineResult<Transaction> {
        match self.datastore.retrieve_transaction(transaction_id)? {
            Some(referenced_transaction) => Ok(referenced_transaction),
            None => Err(PaymentEngineError::DisputedTransactionNotFound),
        }
    }

    fn save_account_to_datastore(&mut self, account: &mut Account) -> PaymentEngineResult<()> {
        account.round_values();
        self.datastore.save_account(account.clone())?;

        Ok(())
    }

    fn retrieve_account(&self, client_id: u16) -> PaymentEngineResult<Account> {
        match self.datastore.retrieve_account(client_id)? {
            None => Ok(Account::new(client_id)),
            Some(account) => Ok(account),
        }
    }

    fn write_accounts(&self) -> PaymentEngineResult<()> {
        let accounts = self.datastore.retrieve_all_accounts()?;
        let mut writer = WriterBuilder::new().from_writer(std::io::stdout());

        for account in accounts {
            writer.serialize(account)?;
        }

        writer.flush()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::datastore::DatastoreOperations;
    use crate::error::PaymentEngineResult;
    use crate::model::{Account, Transaction, TransactionType};
    use crate::payment_service::PaymentService;
    use rust_decimal::prelude::*;
    use rust_decimal::Decimal;
    use std::collections::HashMap;

    struct MockDatastore {
        accounts: HashMap<u16, Account>,
        transactions: Vec<Transaction>,
    }

    impl MockDatastore {
        pub fn new(accounts: HashMap<u16, Account>, transactions: Vec<Transaction>) -> Self {
            MockDatastore {
                accounts,
                transactions,
            }
        }
    }

    impl DatastoreOperations for MockDatastore {
        fn retrieve_transaction(
            &mut self,
            transaction_id: u32,
        ) -> PaymentEngineResult<Option<Transaction>> {
            Ok(self
                .transactions
                .iter()
                .find(|t| t.transaction_id == transaction_id)
                .cloned())
        }

        fn save_transaction(&mut self, transaction: Transaction) -> PaymentEngineResult<()> {
            self.transactions.push(transaction);
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
            let transaction = self
                .transactions
                .iter_mut()
                .find(|t| t.transaction_id == transaction_id)
                .unwrap();

            transaction.disputed = disputed;

            Ok(())
        }

        fn remove_transaction_from_cache(
            &mut self,
            _transaction_id: u32,
        ) -> PaymentEngineResult<()> {
            Ok(())
        }
    }

    #[test]
    pub fn should_deposit_account() {
        let datastore = MockDatastore::new(HashMap::default(), vec![]);
        let mut service = PaymentService::new(Box::new(datastore));
        let client_id = 1;

        let transaction = Transaction {
            r#type: TransactionType::Deposit,
            client_id,
            transaction_id: 1,
            amount: Option::from(Decimal::from(500)),
            disputed: false,
        };

        let mut account = Account {
            client_id,
            available: Default::default(),
            held: Default::default(),
            total: Default::default(),
            locked: false,
        };

        service.handle_deposit(&transaction, &mut account).unwrap();

        let account = service.retrieve_account(1).unwrap();

        assert_eq!(account.available, Decimal::from(500));
        assert_eq!(account.total, Decimal::from(500));
        assert_eq!(account.held, Decimal::ZERO);
    }

    #[test]
    pub fn should_withdraw_account() {
        let datastore = MockDatastore::new(HashMap::default(), vec![]);
        let mut service = PaymentService::new(Box::new(datastore));
        let client_id = 2;

        let transaction = Transaction {
            r#type: TransactionType::Withdrawal,
            client_id,
            transaction_id: 2,
            amount: Option::from(Decimal::from(500)),
            disputed: false,
        };

        let mut account = Account {
            client_id,
            available: Decimal::from(1000),
            held: Default::default(),
            total: Decimal::from(1000),
            locked: false,
        };

        service
            .handle_withdrawal(&transaction, &mut account)
            .unwrap();

        let account = service.retrieve_account(client_id).unwrap();

        assert_eq!(account.available, Decimal::from(500));
        assert_eq!(account.total, Decimal::from(500));
        assert_eq!(account.held, Decimal::ZERO);
    }

    #[test]
    pub fn should_dispute_transaction_deposit_with_resolution() {
        let datastore = MockDatastore::new(HashMap::default(), vec![]);
        let mut service = PaymentService::new(Box::new(datastore));
        let client_id = 3;

        let transaction = Transaction {
            r#type: TransactionType::Deposit,
            client_id,
            transaction_id: 333,
            amount: Option::from(Decimal::from(500)),
            disputed: false,
        };

        let mut action_transaction = Transaction {
            r#type: TransactionType::Dispute,
            client_id,
            transaction_id: 333,
            amount: None,
            disputed: false,
        };

        let mut account = Account {
            client_id,
            available: Decimal::from(1000),
            held: Default::default(),
            total: Decimal::from(1000),
            locked: false,
        };

        service.handle_deposit(&transaction, &mut account).unwrap();

        let mut account = service.retrieve_account(client_id).unwrap();

        assert_eq!(account.held, Decimal::ZERO);
        assert_eq!(account.available, Decimal::from(1500));
        assert_eq!(account.total, Decimal::from(1500));

        service
            .handle_dispute(&action_transaction, &mut account)
            .unwrap();

        let mut account = service.retrieve_account(client_id).unwrap();

        assert_eq!(account.available, Decimal::from(1000));
        assert_eq!(account.total, Decimal::from(1500));
        assert_eq!(account.held, Decimal::from(500));

        action_transaction.r#type = TransactionType::Resolve;

        service
            .handle_resolve(&action_transaction, &mut account)
            .unwrap();

        let account = service.retrieve_account(client_id).unwrap();

        assert_eq!(account.available, Decimal::from(1500));
        assert_eq!(account.total, Decimal::from(1500));
        assert_eq!(account.held, Decimal::ZERO);
    }

    #[test]
    pub fn should_dispute_transaction_withdrawal_with_resolution() {
        let datastore = MockDatastore::new(HashMap::default(), vec![]);
        let mut service = PaymentService::new(Box::new(datastore));
        let client_id = 3;

        let transaction = Transaction {
            r#type: TransactionType::Withdrawal,
            client_id,
            transaction_id: 455,
            amount: Option::from(Decimal::from(500)),
            disputed: false,
        };

        let mut action_transaction = Transaction {
            r#type: TransactionType::Dispute,
            client_id,
            transaction_id: 455,
            amount: None,
            disputed: false,
        };

        let mut account = Account {
            client_id,
            available: Decimal::from(1000),
            held: Default::default(),
            total: Decimal::from(1000),
            locked: false,
        };

        service
            .handle_withdrawal(&transaction, &mut account)
            .unwrap();

        let mut account = service.retrieve_account(client_id).unwrap();

        assert_eq!(account.held, Decimal::ZERO);
        assert_eq!(account.available, Decimal::from(500));
        assert_eq!(account.total, Decimal::from(500));

        service
            .handle_dispute(&action_transaction, &mut account)
            .unwrap();

        let mut account = service.retrieve_account(client_id).unwrap();

        assert_eq!(account.available, Decimal::from(500));
        assert_eq!(account.total, Decimal::from(1000));
        assert_eq!(account.held, Decimal::from(500));

        action_transaction.r#type = TransactionType::Resolve;

        service
            .handle_resolve(&action_transaction, &mut account)
            .unwrap();

        let account = service.retrieve_account(client_id).unwrap();

        assert_eq!(account.available, Decimal::from(1000));
        assert_eq!(account.total, Decimal::from(1000));
        assert_eq!(account.held, Decimal::ZERO);
    }

    #[test]
    pub fn should_chargeback_account() {
        let datastore = MockDatastore::new(HashMap::default(), vec![]);
        let mut service = PaymentService::new(Box::new(datastore));
        let client_id = 3;

        let transaction = Transaction {
            r#type: TransactionType::Withdrawal,
            client_id,
            transaction_id: 455,
            amount: Option::from(Decimal::from(500)),
            disputed: false,
        };

        let mut action_transaction = Transaction {
            r#type: TransactionType::Dispute,
            client_id,
            transaction_id: 455,
            amount: None,
            disputed: false,
        };

        let account = Account {
            client_id,
            available: Decimal::from(1000),
            held: Default::default(),
            total: Decimal::from(1000),
            locked: false,
        };

        service
            .handle_withdrawal(&transaction, &mut account.clone())
            .unwrap();

        let mut account = service.retrieve_account(client_id).unwrap();

        assert_eq!(account.held, Decimal::ZERO);
        assert_eq!(account.available, Decimal::from(500));
        assert_eq!(account.total, Decimal::from(500));

        service
            .handle_dispute(&action_transaction, &mut account)
            .unwrap();

        let mut account = service.retrieve_account(client_id).unwrap();

        assert_eq!(account.available, Decimal::from(500));
        assert_eq!(account.total, Decimal::from(1000));
        assert_eq!(account.held, Decimal::from(500));

        action_transaction.r#type = TransactionType::Chargeback;

        service
            .handle_chargeback(&action_transaction, &mut account)
            .unwrap();

        let account = service.retrieve_account(client_id).unwrap();

        assert_eq!(account.available, Decimal::from(500));
        assert_eq!(account.total, Decimal::from(500));
        assert_eq!(account.held, Decimal::ZERO);
        assert_eq!(account.locked, true);
    }

    #[test]
    pub fn should_process_transactions_from_csv() {
        let datastore = MockDatastore::new(HashMap::default(), vec![]);
        let mut service = PaymentService::new(Box::new(datastore));

        match service.run("test.csv") {
            Ok(_) => {}
            Err(_) => {
                assert!(false)
            }
        };

        let account = service.retrieve_account(1).unwrap();

        assert_eq!(account.available, from_str_to_decimal("400.9699"));
        assert_eq!(account.held, from_str_to_decimal("600"));
        assert_eq!(account.total, from_str_to_decimal("1000.9699"));
        assert_eq!(account.locked, false);

        let account = service.retrieve_account(2).unwrap();

        assert_eq!(account.available, from_str_to_decimal("5600"));
        assert_eq!(account.held, from_str_to_decimal("0"));
        assert_eq!(account.total, from_str_to_decimal("5600"));
        assert_eq!(account.locked, true);

        let account = service.retrieve_account(3).unwrap();

        assert_eq!(account.available, from_str_to_decimal("0"));
        assert_eq!(account.held, from_str_to_decimal("500"));
        assert_eq!(account.total, from_str_to_decimal("500"));
        assert_eq!(account.locked, false);

        let account = service.retrieve_account(33).unwrap();

        assert_eq!(account.available, from_str_to_decimal("2500"));
        assert_eq!(account.held, from_str_to_decimal("300"));
        assert_eq!(account.total, from_str_to_decimal("2800"));
        assert_eq!(account.locked, false);

        let account = service.retrieve_account(99).unwrap();

        assert_eq!(account.available, from_str_to_decimal("1000"));
        assert_eq!(account.held, from_str_to_decimal("500"));
        assert_eq!(account.total, from_str_to_decimal("1500"));
        assert_eq!(account.locked, false);
    }

    fn from_str_to_decimal(amount: &str) -> Decimal {
        Decimal::from_str(amount).unwrap()
    }
}
