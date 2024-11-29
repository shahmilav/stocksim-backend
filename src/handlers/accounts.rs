use crate::auth::GoogleUserInfo;
use crate::db::DatabasePool;
use crate::finnhub::fetch_stock_price;
use crate::models::Account;
use axum::{extract::State, http::StatusCode, Json};
use tower_sessions::Session;

#[axum::debug_handler]
/// Gets an account by ID.
pub async fn get_account(
    State(pool): State<DatabasePool>,
    session: Session,
) -> Result<(StatusCode, Json<Account>), (StatusCode, Json<String>)> {
    let sess: GoogleUserInfo = session.get("SESSION").await.unwrap().unwrap_or_default();
    let s = sess.email;

    // Validate that this session exists!
    if s.is_empty() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json("Unauthorized access".to_string()),
        ));
    }

    // Lock the connection
    let conn = pool.0.lock().await;

    // Fetch account details
    let mut account = {
        let mut stmt = conn
            .prepare("SELECT id, value, cash FROM accounts WHERE id = ?")
            .unwrap();

        stmt.query_row([&s], |row| {
            Ok(Account {
                id: row.get(0)?,
                value: row.get(1)?,
                cash: row.get(2)?,
                change: 0,
            })
        })
        .unwrap()
    };

    // Fetch holdings
    #[derive(Debug, Clone)]
    struct T {
        stock_symbol: String,
        quantity: i32,
    }
    let holdings: Vec<T> = {
        let mut stmt = conn
            .prepare("SELECT stock_symbol, quantity FROM holdings WHERE account_id = ?")
            .unwrap();

        stmt.query_map([&s], |row| {
            Ok(T {
                stock_symbol: row.get(0)?,
                quantity: row.get(1)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<T>, _>>()
        .unwrap()
    };

    // Process stock price changes
    let mut sumchanges = 0;
    for h in holdings.iter() {
        match fetch_stock_price(&h.stock_symbol).await {
            Ok(quote) => {
                let current_value = (quote.c * 100.0) as i32 * h.quantity;
                let yesterday = (quote.pc * 100.0) as i32 * h.quantity;
                let change = current_value - yesterday;
                sumchanges += change;
            }
            Err(_) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json("Failed to fetch stock price".to_string()),
                ));
            }
        }
    }

    // Update account change
    account.change = sumchanges;

    // Return account if found
    Ok((StatusCode::OK, Json(account)))
}


