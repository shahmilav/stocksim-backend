use crate::db::DatabasePool;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{extract::Query, response::Redirect, Json};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use tower_sessions::Session;
use url::Url;

/// Start the Google login flow by redirecting the user to the Google login page.
pub async fn start_google_login() -> Redirect {
    let client_id = env::var("GOOGLE_CLIENT_ID").expect("Missing GOOGLE_CLIENT_ID");
    let redirect_uri = env::var("GOOGLE_REDIRECT_URI").expect("Missing GOOGLE_REDIRECT_URI");

    let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
    url.query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "openid email profile")
        .append_pair("access_type", "offline");

    Redirect::temporary(url.as_str())
}

/// Handle the callback from Google after the user logs in.
pub async fn handle_google_callback(
    session: Session,
    State(pool): State<DatabasePool>,
    Query(params): Query<GoogleCallbackQuery>,
) -> Redirect {
    let client = Client::new();

    let client_id = env::var("GOOGLE_CLIENT_ID").expect("Missing GOOGLE_CLIENT_ID");
    let client_secret = env::var("GOOGLE_CLIENT_SECRET").expect("Missing GOOGLE_CLIENT_SECRET");
    let redirect_uri = env::var("GOOGLE_REDIRECT_URI").expect("Missing GOOGLE_REDIRECT_URI");

    // Exchange authorization code for access token
    let token_resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", params.code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .unwrap()
        .json::<GoogleTokenResponse>()
        .await
        .unwrap();

    // Use the access token to get user info
    let user_info_resp = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&token_resp.access_token)
        .send()
        .await
        .unwrap()
        .json::<GoogleUserInfo>()
        .await
        .unwrap();

    let account = pool
        .get_account(&user_info_resp.email.to_string())
        .await
        .unwrap_or_default()
        .unwrap_or_default();

    if account.id == "" {
        pool.add_account(crate::models::Account {
            id: user_info_resp.email.to_string(),
            cash: 100000_00,
            value: 100000_00,
            change: 0,
        })
        .await
        .unwrap();
    }

    match session.insert("SESSION", user_info_resp).await {
        Ok(_) => {
            tracing::info!("Session inserted");
        }
        Err(e) => {
            tracing::error!("Error inserting session: {:?}", e);
        }
    }
    Redirect::temporary("http://localhost:5173/home")
}

/// Logout the user by removing the session.
pub async fn logout(session: Session) -> Redirect {
    session.remove::<GoogleUserInfo>("SESSION").await.unwrap();
    session.flush().await.unwrap();

    Redirect::to("http://localhost:5173")
}

/// Get user data from the session.
pub async fn get_user_data(
    session: Session,
) -> Result<(StatusCode, Json<GoogleUserInfo>), StatusCode> {
    match validate_session(session).await {
        Ok(info) => Ok((StatusCode::OK, Json(info))),
        Err(status) => Err(status),
    }
}

/// Validate the session and return the user info if valid.
pub async fn validate_session(session: Session) -> Result<GoogleUserInfo, StatusCode> {
    let info: GoogleUserInfo = session.get("SESSION").await.unwrap().unwrap_or_default();
    if info.email.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(info)
}

/// Query parameters sent by Google during the callback.
#[derive(Debug, Deserialize)]
pub struct GoogleCallbackQuery {
    code: String,
}

/// Response from Google's token endpoint.
#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
    access_token: String,
}

/// User info retrieved from Google's API.
#[derive(Debug, Serialize, Deserialize)]
pub struct GoogleUserInfo {
    pub(crate) email: String,
    pub(crate) name: String,
    pub(crate) picture: String,
}

/// Default implementation for GoogleUserInfo. All fields are empty strings.
impl Default for GoogleUserInfo {
    fn default() -> Self {
        GoogleUserInfo {
            email: "".to_string(),
            name: "".to_string(),
            picture: "".to_string(),
        }
    }
}
