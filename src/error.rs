#[derive(Debug, Display, Error, From)]
#[display(fmt = "PaymentEngine error: {}")]
pub enum PaymentEngineError {
    #[display(fmt = "Error importing CSV file, check file path")]
    CsvImport { source: csv::Error },
    #[display(fmt = "Error exporting CSV file")]
    CsvExport { source: std::io::Error },
    #[display(fmt = "No amount for transaction which requires it")]
    NoAmount,
    #[display(fmt = "There are not enough funds on the account")]
    InsufficientAccountFunds,
    #[display(fmt = "Disputed transaction does not exist")]
    DisputedTransactionNotFound,
    #[display(
        fmt = "Invalid disputed transaction, dispute can only be done for withdrawal and deposit"
    )]
    InvalidDisputedTransactionType,
    #[display(fmt = "Transaction is already disputed")]
    TransactionAlreadyDisputed,
    #[display(fmt = "Transaction not found, cannot change disputed status")]
    DisputedValueChange,
    #[display(fmt = "Transaction is not disputed")]
    TransactionNotDisputed,
    #[display(fmt = "Cannot serialize/deserialize JSON")]
    Json { source: serde_json::Error },
    #[display(fmt = "Cannot read/save data with pickle_db")]
    PickleDb { source: pickledb::error::Error },
}

pub type PaymentEngineResult<T> = Result<T, PaymentEngineError>;
