use actix_web::{get, http::header, web, App, HttpResponse, HttpServer, Responder};
use dotenv::dotenv;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;

/// A single post from "app.bsky.feed.searchPosts".
/// We capture common fields plus a generic `extra` map for anything unknown.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BskyPost {
    uri: String,
    cid: Option<String>, // new field
    #[serde(rename = "indexedAt")]
    indexed_at: String,
    author: Option<BskyAuthor>, // we can parse author details
    record: BskyPostRecord,     // the actual text, timestamps, etc.
    #[serde(rename = "repostCount")]
    repost_count: Option<u32>, // example extra fields
    #[serde(rename = "replyCount")]
    reply_count: Option<u32>,
    #[serde(rename = "likeCount")]
    like_count: Option<u32>,
    #[serde(rename = "quoteCount")]
    quote_count: Option<u32>,

    /// Some fields like "viewer", "labels", or "embed" can be typed further,
    /// but for illustration, let's store them generically here.
    #[serde(default)]
    viewer: Value,
    #[serde(default)]
    labels: Value,
    #[serde(default)]
    embed: Value,

    /// Flatten any fields we didn’t explicitly define so we don’t lose them.
    /// This makes debugging easier if new fields appear in the JSON.
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

/// The “author” sub-object (e.g., who posted it).
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BskyAuthor {
    did: Option<String>,
    handle: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    avatar: Option<String>,

    #[serde(default)]
    associated: Value, // e.g. "associated": { ... }
    #[serde(default)]
    labels: Value, // could be typed further

    // Flatten anything else
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

/// The “record” part of each post (contains the main text, facets, etc.).
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BskyPostRecord {
    /// This is often present in Bluesky objects:
    #[serde(rename = "$type")]
    record_type: Option<String>,

    text: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,

    #[serde(default)]
    embed: Value, // could be typed if you need the "app.bsky.embed.*" shape
    #[serde(default)]
    facets: Value, // array of link/mention/tag references
    #[serde(default)]
    langs: Value, // array of language codes
    #[serde(default)]
    reply: Value, // sometimes has the parent & root replies

    // Flatten anything else (like "text", "createdAt", etc. we didn't define)
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

/// The top-level structure for the "searchPosts" response
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BskySearchPostsResponse {
    #[serde(default)]
    posts: Vec<BskyPost>,

    /// For pagination, if present
    #[serde(default)]
    cursor: Option<String>,

    // Flatten anything else
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

/// Represents the session token retrieved from Bluesky login.
#[derive(Debug, Deserialize)]
struct BskySession {
    #[serde(rename = "accessJwt")]
    access_jwt: String,
}

async fn bluesky_login(client: &Client) -> Result<String, Box<dyn std::error::Error>> {
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

/// Searches Bluesky posts by a naive hashtag approach.
async fn search_bluesky_posts(
    client: &Client,
    token: &str,
    hashtags: &[String],
    max_posts: usize,
) -> Result<Vec<BskyPost>, Box<dyn std::error::Error>> {
    let base_url = env::var("BLUESKY_BASE_URL").unwrap_or_else(|_| "https://bsky.social".to_string());

    // e.g. "#rust #actix #web"
    let joined_query = hashtags.iter().map(|tag| format!("#{}", tag)).collect::<Vec<_>>().join(" ");

    let limit = max_posts.min(50);
    let url = format!("{}/xrpc/app.bsky.feed.searchPosts", base_url);

    let resp = client
        .get(url)
        .bearer_auth(token)
        .query(&[("q", &joined_query), ("limit", &limit.to_string())])
        .send()
        .await?
        .error_for_status()?;

    // Get the raw JSON (for debugging, if needed)
    let text = resp.text().await?;
    // println!("DEBUG: raw response body: {text}");

    let mut result: BskySearchPostsResponse = serde_json::from_str(&text)?;
    // Sort descending by indexed_at
    result.posts.sort_by_key(|p| p.indexed_at.clone());
    result.posts.reverse();

    Ok(result.posts)
}

#[get("/")]
async fn index(query: web::Query<HashMap<String, String>>) -> impl Responder {
    // A simple HTML skeleton
    let mut body = String::from(
        r#"
        <p class="size-h1">Parameters:</p>
    "#,
    );

    // Show all query parameters
    for (key, value) in query.iter() {
        body.push_str(&format!("<p><strong>{}:</strong> {}</p>", key, value));
    }

    // Extract tags=... and limit=...
    let tags_param = query.get("tags").cloned().unwrap_or_default();
    let limit_param = query.get("limit").and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);

    // Split comma-separated tags
    let tags: Vec<String> = tags_param
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .collect();

    if tags.is_empty() {
        body.push_str("<p>No tags specified. Try ?tags=rust,actix</p>");
        return HttpResponse::Ok()
            .insert_header(("Widget-Title", "Test"))
            .insert_header(("Widget-Content-Type", "html"))
            .insert_header(header::ContentType::html())
            .body(body);
    }

    // Create the HTTP client
    let client = Client::new();

    // Attempt to log in and then search
    match bluesky_login(&client).await {
        Ok(token) => match search_bluesky_posts(&client, &token, &tags, limit_param).await {
            Ok(posts) => {
                if posts.is_empty() {
                    body.push_str("<p>No posts found for those hashtags.</p>");
                } else {
                    body.push_str("<h2>Recent Posts</h2>");

                    // Render each post
                    for post in &posts {
                        let text = post.record.text.clone().unwrap_or_else(|| "<no text>".to_string());
                        let handle = post
                            .author
                            .as_ref()
                            .and_then(|auth| auth.handle.clone())
                            .unwrap_or_else(|| "Unknown".to_string());

                        body.push_str(&format!(
                            r#"<div style="margin-bottom: 1em; padding: 0.5em; border: 1px solid #ccc;">
                                <p><strong>Handle:</strong> {}</p>
                                <p><strong>URI:</strong> {}</p>
                                <p><strong>Indexed At:</strong> {}</p>
                                <p><strong>Text:</strong> {}</p>
                                <p><strong>Extra:</strong> {:?}</p>
                                <p><a href="{}">View on Bluesky</a></p>
                            </div>"#,
                            handle, post.uri, post.indexed_at, text, post.extra, post.uri
                        ));
                    }
                }
            }
            Err(e) => {
                body.push_str(&format!("<p>Error searching posts: {}</p>", e));
            }
        },
        Err(e) => {
            body.push_str(&format!("<p>Error logging into Bluesky: {}</p>", e));
        }
    }

    HttpResponse::Ok()
        .insert_header(("Widget-Title", "Test"))
        .insert_header(("Widget-Content-Type", "html"))
        .insert_header(header::ContentType::html())
        .body(body)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    HttpServer::new(|| App::new().service(index)).bind(("0.0.0.0", 8080))?.run().await
}
