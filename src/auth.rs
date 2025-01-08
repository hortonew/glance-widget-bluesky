use actix_web::web;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

const TOKEN_FILE: &str = "bluesky_tokens.json";

/// Represents the session token retrieved from Bluesky login.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BskySession {
    #[serde(rename = "accessJwt")]
    pub access_jwt: String,
    #[serde(rename = "refreshJwt")]
    pub refresh_jwt: String,
    pub did: String,
}

/// A small struct to hold our Bluesky token in an Arc<Mutex> so we can share it.
#[derive(Clone)]
pub struct BskyState {
    pub token: Arc<Mutex<Option<BskySession>>>,
}

pub fn save_tokens(session: &BskySession) {
    let json = serde_json::to_string(session).expect("Failed to serialize token data");
    fs::write(TOKEN_FILE, json).expect("Failed to write token file");
}

pub fn load_tokens() -> Option<BskySession> {
    if Path::new(TOKEN_FILE).exists() {
        let json = fs::read_to_string(TOKEN_FILE).expect("Failed to read token file");
        serde_json::from_str(&json).ok()
    } else {
        None
    }
}

pub async fn bluesky_login(client: &Client) -> Result<BskySession, Box<dyn std::error::Error>> {
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
    save_tokens(&session);
    println!("Logged in and obtained new token.");
    Ok(session)
}

pub async fn refresh_access_token(refresh_jwt: &str) -> Option<BskySession> {
    #[derive(Serialize)]
    struct RefreshRequest {
        #[serde(rename = "refreshJwt")]
        refresh_jwt: String,
    }

    #[derive(Deserialize)]
    struct RefreshResponse {
        #[serde(rename = "accessJwt")]
        access_jwt: String,
        #[serde(rename = "refreshJwt")]
        refresh_jwt: String,
        did: String,
    }

    let client = Client::new();
    let refresh_data = RefreshRequest {
        refresh_jwt: refresh_jwt.to_string(),
    };

    match client
        .post("https://bsky.social/xrpc/com.atproto.server.refreshSession")
        .json(&refresh_data)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                let refresh_response: RefreshResponse = response.json().await.ok()?;
                let session = BskySession {
                    access_jwt: refresh_response.access_jwt,
                    refresh_jwt: refresh_response.refresh_jwt,
                    did: refresh_response.did,
                };
                save_tokens(&session);
                println!("Token refreshed successfully.");
                Some(session)
            } else {
                println!("Failed to refresh access token: {:?}", response.text().await);
                None
            }
        }
        Err(err) => {
            println!("Error refreshing token: {:?}", err);
            None
        }
    }
}

pub async fn ensure_bsky_token(client: &Client, data: &web::Data<BskyState>, body: &mut String) -> Option<String> {
    let mut token_guard = data.token.lock().await;
    if let Some(session) = token_guard.as_ref() {
        // Check if the token is still valid
        if is_token_valid(&session.access_jwt).await {
            println!("Using existing valid token.");
            return Some(session.access_jwt.clone());
        }
        // Try to refresh the token
        if let Some(new_session) = refresh_access_token(&session.refresh_jwt).await {
            *token_guard = Some(new_session.clone());
            println!("Token was expired and has been refreshed.");
            return Some(new_session.access_jwt);
        }
    }
    // If no valid token, perform login
    match bluesky_login(client).await {
        Ok(session) => {
            *token_guard = Some(session.clone());
            println!("No valid token found, logged in to obtain a new token.");
            Some(session.access_jwt)
        }
        Err(e) => {
            body.push_str(&format!("<p>Error logging into Bluesky: {}</p>", e));
            None
        }
    }
}

async fn is_token_valid(token: &str) -> bool {
    // Implement a simple check to see if the token is still valid
    // For example, you could make a request to a Bluesky endpoint that requires authentication
    // and check if it returns a 401 Unauthorized status code
    let client = Client::new();
    let url = "https://bsky.social/xrpc/app.bsky.feed.getTimeline"; // Example endpoint
    let resp = client.get(url).bearer_auth(token).send().await;
    match resp {
        Ok(response) => response.status() != 401,
        Err(_) => false,
    }
}
