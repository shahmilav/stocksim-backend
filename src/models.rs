use serde::{Deserialize, Serialize};

/// Account represents a user's account.
/// It has an id, total value, and cash.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Account {
    pub id: String,
    pub value: i32,
    pub cash: i32,
    pub change: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateAccount {
    pub value: i32,
    pub cash: i32,
}
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Holding {
    pub stock_symbol: String,
    pub stock_name: String,
    pub quantity: i32,
    pub current_price: i32,
    pub total_value: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HoldingResponse {
    pub stock_symbol: String,
    pub stock_name: String,
    pub quantity: i32,
    pub current_price: i32,
    pub total_value: i32,
    pub day_change: i32,
    pub day_change_percent: i32,
    pub purchase_price: i32,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Portfolio {
    pub holdings: Vec<HoldingResponse>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TradeRequest {
    pub stock_symbol: String,
    pub quantity: i32,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Transaction {
    pub id: String,
    pub account_id: String,
    pub stock_symbol: String,
    pub transaction_type: String,
    pub quantity: i32,
    pub price: i32,
    pub timestamp: String,
}
