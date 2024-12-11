use crate::auth::validate_session;
use crate::db::DatabasePool;
use crate::finnhub::{fetch_stock_price, fetch_stock_profile};
use crate::models::{HoldingResponse, Portfolio, Transaction};
use axum::{extract::State, http::StatusCode, Json};
use tower_sessions::Session;

pub async fn get_portfolio(
    session: Session,
    State(pool): State<DatabasePool>,
) -> Result<(StatusCode, Json<Portfolio>), (StatusCode, Json<String>)> {
    // Validate the session
    let info = match validate_session(session).await {
        Ok(info) => info,
        Err(status) => return Err((status, Json("Unauthorized access".to_string()))),
    };
    let account_id = info.email;

    // Use the `get_holdings` method
    let holdings = match pool.get_holdings(&account_id).await {
        Ok(holdings) => holdings,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Failed to fetch holdings: {}", e)),
            ));
        }
    };

    let mut h: Vec<HoldingResponse> = Vec::new();
    for holding in holdings {
        h.push(HoldingResponse {
            stock_symbol: holding.stock_symbol,
            stock_name: holding.stock_name,
            quantity: holding.quantity,
            current_price: holding.current_price,
            total_value: holding.total_value,
            day_change: 0,
            day_change_percent: 0,
            purchase_price: holding.purchase_price,
            stock_logo_url: String::from(""),
            overall_change: 0,
            category: String::from(""),
        });
    }

    let mut updated_holdings = Vec::new();
    let mut total_portfolio_value = 0;

    for mut holding in h {
        // Fetch stock price and update holding
        match fetch_stock_price(&holding.stock_symbol).await {
            Ok(quote) => {
                let current_price = (quote.c * 100.0) as i32;
                let total_value = current_price * holding.quantity;
                holding.current_price = current_price;
                holding.total_value = total_value;
                holding.overall_change = total_value - (holding.purchase_price * holding.quantity);
                holding.day_change = (quote.d * 100.0) as i32;
                holding.day_change_percent = (quote.dp * 100.0) as i32;

                total_portfolio_value += total_value;
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(format!("Failed to fetch stock price: {}", e)),
                ));
            }
        }

        // Fetch stock profile for logo and category
        if let Ok(profile) = fetch_stock_profile(&holding.stock_symbol).await {
            holding.stock_logo_url = profile.logo;
            holding.category = profile.finnhub_industry;
        }

        updated_holdings.push(holding);
    }

    let account = match pool.get_account(&account_id).await {
        Ok(account) => account,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Failed to fetch account details: {}", e)),
            ));
        }
    }
    .unwrap();
    // todo: Update the account value in the database

    pool.update_account(
        &account_id,
        (account.cash + total_portfolio_value) as i64,
        account.cash as i64,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(format!("Failed to update account: {}", e)),
        )
    })?;

    // Return the portfolio
    Ok((
        StatusCode::OK,
        Json(Portfolio {
            holdings: updated_holdings,
        }),
    ))
}

pub async fn get_transaction_history(
    session: Session,
    State(pool): State<DatabasePool>,
) -> Result<(StatusCode, Json<Vec<Transaction>>), (StatusCode, Json<String>)> {
    // Validate the session
    let info = match validate_session(session).await {
        Ok(info) => info,
        Err(status) => return Err((status, Json("Unauthorized access".to_string()))),
    };
    let account_id = info.email;

    // Use the `get_transactions` method
    let transactions = match pool.get_transactions(&account_id).await {
        Ok(transactions) => transactions,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Failed to fetch transactions: {}", e)),
            ));
        }
    };

    Ok((StatusCode::OK, Json(transactions)))
}
