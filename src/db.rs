use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct DatabasePool(pub Arc<Mutex<rusqlite::Connection>>);

impl DatabasePool {
    /// Create a new database connection pool.
    pub fn new() -> Result<Self, rusqlite::Error> {
        let conn = rusqlite::Connection::open("db.sqlite")?;

        // Initialize schema for accounts
        conn.execute(
            "CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                value INTEGER NOT NULL,
                cash INTEGER NOT NULL
            )",
            [],
        )?;

        // Initialize schema for portfolio (holdings)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS holdings (
                account_id TEXT NOT NULL,
                stock_symbol TEXT NOT NULL,
                stock_name TEXT NOT NULL,
                quantity INTEGER NOT NULL,
                purchase_price INTEGER NOT NULL,
                PRIMARY KEY (account_id, stock_symbol),
                FOREIGN KEY (account_id) REFERENCES accounts(id)
            )",
            [],
        )?;

        // Initialize schema for transactions
        conn.execute(
            "CREATE TABLE IF NOT EXISTS transactions (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                stock_symbol TEXT NOT NULL,
                transaction_type TEXT NOT NULL,
                quantity INTEGER NOT NULL,
                price INTEGER NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (account_id) REFERENCES accounts(id)
            )",
            [],
        )?;

        Ok(Self(Arc::new(Mutex::new(conn))))
    }
}
