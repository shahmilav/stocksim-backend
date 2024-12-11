use crate::models::{Account, Holding, Transaction};
use futures_util::TryStreamExt;
use mongodb::{
    bson::doc,
    options::{ClientOptions, ServerApi, ServerApiVersion},
    Client, Collection,
};

#[derive(Clone)]
pub struct DatabasePool {
    pub accounts: Collection<Account>,
    pub holdings: Collection<Holding>,
    pub transactions: Collection<Transaction>,
    pub client: Client,
}

impl DatabasePool {
    /// Create a new MongoDB connection pool.
    pub async fn new(uri: &str) -> Result<Self, mongodb::error::Error> {
        let mut options = ClientOptions::parse(uri).await?;
        let server_api = ServerApi::builder().version(ServerApiVersion::V1).build();
        options.server_api = Some(server_api);

        let client = Client::with_options(options)?;

        let db = client.database("user_data");

        db.run_command(doc! { "ping": 1 }).await?;
        tracing::info!("Connected to MongoDB");

        Ok(Self {
            accounts: db.collection::<Account>("accounts"),
            holdings: db.collection::<Holding>("holdings"),
            transactions: db.collection::<Transaction>("transactions"),
            client,
        })
    }

    pub async fn add_account(&self, account: Account) -> Result<(), mongodb::error::Error> {
        self.accounts.insert_one(account).await?;
        Ok(())
    }

    pub async fn get_account(
        &self,
        account_id: &str,
    ) -> Result<Option<Account>, mongodb::error::Error> {
        let filter = doc! { "id": account_id };
        let accounts = &self.accounts;
        let account = accounts.find_one(filter).await?;
        Ok(account)
    }
    pub async fn update_account(
        &self,
        account_id: &str,
        new_value: i64,
        new_cash: i64,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "id": account_id };
        let update = doc! {
            "$set": {
                "value": new_value,
                "cash": new_cash
            }
        };
        let accounts = &self.accounts;
        accounts.update_one(filter, update).await?;
        Ok(())
    }
    pub async fn _delete_account(&self, account_id: &str) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "id": account_id };
        let accounts = &self.accounts;
        accounts.delete_one(filter).await?;
        Ok(())
    }

    pub async fn add_holding(&self, holding: Holding) -> Result<(), mongodb::error::Error> {
        self.holdings.insert_one(holding).await?;
        Ok(())
    }
    pub async fn get_holding(
        &self,
        account_id: &str,
        stock_symbol: &str,
    ) -> Result<Option<Holding>, mongodb::error::Error> {
        let filter = doc! { "account_id": account_id, "stock_symbol": stock_symbol };
        let holdings = &self.holdings;
        let holding = holdings.find_one(filter).await?;
        Ok(holding)
    }

    pub async fn get_holdings(
        &self,
        account_id: &str,
    ) -> Result<Vec<Holding>, mongodb::error::Error> {
        let filter = doc! { "account_id": account_id };
        let x = &self.holdings;
        let cursor = x.find(filter).await?;
        let holdings: Vec<Holding> = cursor.try_collect().await?;
        Ok(holdings)
    }
    pub async fn update_holding(
        &self,
        account_id: &str,
        stock_symbol: &str,
        quantity: i64,
        purchase_price: i64,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "account_id": account_id, "stock_symbol": stock_symbol };
        let update = doc! {
            "$set": {
                "quantity": quantity,
                "purchase_price": purchase_price
            }
        };
        let holdings = &self.holdings;
        holdings.update_one(filter, update).await?;
        Ok(())
    }
    pub async fn delete_holding(
        &self,
        account_id: &str,
        stock_symbol: &str,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "account_id": account_id, "stock_symbol": stock_symbol };
        self.holdings.delete_one(filter).await?;
        Ok(())
    }
    pub async fn add_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<(), mongodb::error::Error> {
        self.transactions.insert_one(transaction).await?;
        Ok(())
    }
    pub async fn get_transactions(
        &self,
        account_id: &str,
    ) -> Result<Vec<Transaction>, mongodb::error::Error> {
        let filter = doc! { "account_id": account_id };
        let transactions = &self.transactions;
        let cursor = transactions.find(filter).await?;
        let transactions: Vec<Transaction> = cursor.try_collect().await?;
        Ok(transactions)
    }
}
