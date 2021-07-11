use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

const DECIMAL_POINT: u32 = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash, Eq)]
pub struct Transaction {
    #[serde(deserialize_with = "transaction_type_deserializer")]
    pub r#type: TransactionType,
    #[serde(alias = "client")]
    pub client_id: u16,
    #[serde(alias = "tx")]
    pub transaction_id: u32,
    #[serde(deserialize_with = "amount_deserializer")]
    pub amount: Option<Decimal>,
    #[serde(default = "default_disputed")]
    pub disputed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash, Eq, Default)]
pub struct Account {
    #[serde(rename = "client")]
    pub client_id: u16,
    pub available: Decimal,
    pub held: Decimal,
    pub total: Decimal,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

impl Account {
    pub fn new(client: u16) -> Self {
        Account {
            client_id: client,
            ..Account::default()
        }
    }

    pub fn round_values(&mut self) {
        self.available = self.available.round_dp(DECIMAL_POINT);
        self.held = self.held.round_dp(DECIMAL_POINT);
        self.total = self.total.round_dp(DECIMAL_POINT);
    }
}

fn amount_deserializer<'de, D>(deserializer: D) -> Result<Option<Decimal>, D::Error>
where
    D: Deserializer<'de>,
{
    let amount_text: &str = Deserialize::deserialize(deserializer)?;

    if amount_text.is_empty() {
        return Ok(None);
    }

    match Decimal::from_str(amount_text) {
        Ok(amount) => {
            if amount.is_zero() {
                Ok(None)
            } else {
                Ok(Option::from(amount.round_dp(DECIMAL_POINT)))
            }
        }
        Err(_) => Err(Error::custom(format!(
            "value \'{}\' cannot be converted to decimal",
            amount_text
        ))),
    }
}

fn transaction_type_deserializer<'de, D>(deserializer: D) -> Result<TransactionType, D::Error>
where
    D: Deserializer<'de>,
{
    let type_text: &str = Deserialize::deserialize(deserializer)?;
    let transaction_type = match type_text.to_lowercase().as_str() {
        "deposit" => TransactionType::Deposit,
        "withdrawal" => TransactionType::Withdrawal,
        "dispute" => TransactionType::Dispute,
        "resolve" => TransactionType::Resolve,
        "chargeback" => TransactionType::Chargeback,
        _ => {
            return Err(Error::custom(format!(
                "value \'{}\' cannot be converted to a valid transaction type",
                type_text
            )))
        }
    };

    Ok(transaction_type)
}

pub fn default_disputed() -> bool {
    false
}
