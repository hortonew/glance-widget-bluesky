use actix_web::web;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Represents the session token retrieved from Bluesky login.
#[derive(Debug, Deserialize)]
pub struct BskySession {
    #[serde(rename = "accessJwt")]
    pub access_jwt: String,
}

/// A small struct to hold our Bluesky token in an Arc<Mutex> so we can share it.
#[derive(Clone)]
pub struct BskyState {
    pub token: Arc<Mutex<Option<String>>>,
}

pub async fn bluesky_login(client: &Client) -> Result<String, Box<dyn std::error::Error>> {
    // Load creds from environment
    let username = env::var("BLUESKY_USERNAME")?;
    let password = env::var("BLUESKY_PASSWORD")?;
    let base_url = env::var("BLUESKY_BASE_URL").unwrap_or_else(|_| "https://bsky.social".to_string());

    let url = format!("{}/xrpc/com.atproto.server.createSession", base_url);

    // Send the login request
    let resp = client
        .post(url)
        .json(&json!({ "identifier": username, "password": password }))
        .send()
        .await?
        .error_for_status()?;

    // Deserialize to get the session token
    let session: BskySession = resp.json().await?;
    Ok(session.access_jwt)
}

pub async fn ensure_bsky_token(client: &Client, data: &web::Data<BskyState>, body: &mut String) -> Option<String> {
    let mut token_guard = data.token.lock().await;
    if let Some(t) = token_guard.as_ref() {
        return Some(t.clone());
    }
    match bluesky_login(client).await {
        Ok(t) => {
            *token_guard = Some(t.clone());
            Some(t)
        }
        Err(e) => {
            body.push_str(&format!("<p>Error logging into Bluesky: {}</p>", e));
            None
        }
    }
}
