use crate::auth::validate_session;
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
    // Validate the session
    let info = match validate_session(session).await {
        Ok(info) => info,
        Err(status) => return Err((status, Json("Unauthorized access".to_string()))),
    };
    let account_id = info.email;

    // Fetch the account details using `get_account` method
    let account = match pool.get_account(&account_id).await {
        Ok(account) => account,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Failed to fetch account details: {}", e)),
            ));
        }
    };

    // Fetch holdings using `get_holdings` method
    let holdings = match pool.get_holdings(&account_id).await {
        Ok(holdings) => holdings,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Failed to fetch holdings: {}", e)),
            ));
        }
    };

    // Calculate changes based on stock prices
    let mut sum_changes = 0;
    for holding in holdings {
        match fetch_stock_price(&holding.stock_symbol).await {
            Ok(quote) => {
                let current_value = (quote.c * 100.0) as i32 * holding.quantity;
                let yesterday_value = (quote.pc * 100.0) as i32 * holding.quantity;
                sum_changes += current_value - yesterday_value;
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(format!("Failed to fetch stock price: {}", e)),
                ));
            }
        }
    }

    let mut a = account.unwrap();

    // Update the `change` field of the account
    a.change = sum_changes;

    // Return the updated account
    Ok((StatusCode::OK, Json(a)))
}
