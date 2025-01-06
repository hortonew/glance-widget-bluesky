use actix_web::{get, http::header, web, App, HttpResponse, HttpServer, Responder};
use dotenv::dotenv;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;

use chrono::{DateTime, Duration, Utc}; // Add or ensure these are imported

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

/// A small struct to hold our Bluesky token in an Arc<Mutex> so we can share it.
#[derive(Clone)]
struct BskyState {
    token: Arc<Mutex<Option<String>>>,
}

fn parse_relative_time(spec: &str) -> Option<DateTime<Utc>> {
    if !spec.starts_with('-') || spec.len() < 3 {
        return None;
    }

    // Split off the trailing unit (last char), e.g. 'd', 'h', 'm', or 's'
    let (num_part, unit_part) = spec[1..].split_at(spec.len() - 2);
    let digits = num_part.parse::<i64>().ok()?;
    let unit_char = unit_part.chars().next()?;

    let duration = match unit_char {
        'd' => Duration::days(digits),
        'h' => Duration::hours(digits),
        'm' => Duration::minutes(digits),
        's' => Duration::seconds(digits),
        _ => return None,
    };

    // Return "now - duration"
    Some(Utc::now() - duration)
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
    maybe_since_time: Option<DateTime<Utc>>,
) -> Result<Vec<BskyPost>, Box<dyn std::error::Error>> {
    let base_url = env::var("BLUESKY_BASE_URL").unwrap_or_else(|_| "https://bsky.social".to_string());

    // e.g. "#rust #actix #web"
    let base_query = hashtags.iter().map(|tag| format!("#{}", tag)).collect::<Vec<_>>().join(" ");

    // If we got a valid DateTime, prepend it as `since:2025-01-05T12:34:56Z`
    let joined_query = if let Some(since_dt) = maybe_since_time {
        let timestamp_str = since_dt.to_rfc3339(); // e.g. "2025-01-05T12:34:56Z"
        format!("since:{} {}", timestamp_str, base_query)
    } else {
        // If there's no valid 'since' param, just use the base query
        base_query
    };

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
async fn index(query: web::Query<HashMap<String, String>>, data: web::Data<BskyState>) -> impl Responder {
    // Extract tags=... and limit=...
    let tags_param = query.get("tags").cloned().unwrap_or_default();
    let limit_param = query.get("limit").and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);
    let debug_param = query.get("debug").and_then(|s| s.parse::<bool>().ok()).unwrap_or(false);
    let text_color = query
        .get("text_color")
        .and_then(|s| s.parse::<String>().ok())
        .unwrap_or("000000".to_string());
    let author_color = query
        .get("author_color")
        .and_then(|s| s.parse::<String>().ok())
        .unwrap_or("666".to_string());
    let text_hover_color = query
        .get("text_hover_color")
        .and_then(|s| s.parse::<String>().ok())
        .unwrap_or("000000".to_string());
    let author_hover_color = query
        .get("author_hover_color")
        .and_then(|s| s.parse::<String>().ok())
        .unwrap_or("666".to_string());

    // e.g. ?since=-10d
    let since_param = query.get("since").cloned().unwrap_or_default();

    // Try to parse something like "-10d"
    let maybe_since_time = if !since_param.is_empty() {
        parse_relative_time(&since_param)
    } else {
        None
    };

    let mut body = format!(
        r#"<!DOCTYPE html>
    <html>
    <head>
        <meta charset="utf-8"/>
        <title>Bluesky Hashtag Viewer</title>
        <style>
            /* Container around each post */
            .post-container {{
                margin-bottom: 1em;
                padding: 0.5em;
                border: 1px solid #ccc;
                text-align: left; /* ensure left alignment */
            }}
            /* Main post text */
            .post-text {{
                margin: 0;
                font-size: 1em; /* normal font size */
            }}

            .post-text a {{
                color: #{text_color};
                text-decoration: none;
            }}
            .post-text a:hover {{
                color: #{text_hover_color};
                text-decoration: none;
            }}

            /* Author line (small text) */
            .post-author {{
                margin: 0.25em 0 0 0;
                font-size: 0.85em;
                color: #{author_color};
            }}
            .post-author a {{
                color: inherit;
                text-decoration: none;
            }}
            .post-author a:hover {{
                color: #{author_hover_color};
                text-decoration: none;
            }}
        </style>
    </head>
    <body>
    "#,
        text_color = text_color,
        text_hover_color = text_hover_color,
        author_color = author_color,
        author_hover_color = author_hover_color
    );

    if debug_param {
        body.push_str(r#"<p class="size-h1">Parameters:</p>"#);

        // Show all query parameters
        for (key, value) in query.iter() {
            body.push_str(&format!("<p><strong>{}:</strong> {}</p>", key, value));
        }
    }

    // Split comma-separated tags
    let tags: Vec<String> = tags_param
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .collect();

    if tags.is_empty() {
        body.push_str("<p>No tags specified. Try ?tags=rust,actix&limit=5</p>");
        return HttpResponse::Ok()
            .insert_header(("Widget-Title", "Test"))
            .insert_header(("Widget-Content-Type", "html"))
            .insert_header(header::ContentType::html())
            .body(body);
    }

    // Create the HTTP client
    let client = Client::new();

    // Check if we already have a token cached
    let mut token_guard = data.token.lock().await;
    let token = match token_guard.as_ref() {
        Some(t) => t.clone(), // re-use existing token
        None => {
            // Need to log in
            println!("Logging into Bluesky...");
            match bluesky_login(&client).await {
                Ok(t) => {
                    // Cache it
                    *token_guard = Some(t.clone());
                    t
                }
                Err(e) => {
                    body.push_str(&format!("<p>Error logging into Bluesky: {}</p>", e));
                    return HttpResponse::Ok()
                        .insert_header(("Widget-Title", "Test"))
                        .insert_header(("Widget-Content-Type", "html"))
                        .insert_header(header::ContentType::html())
                        .body(body);
                }
            }
        }
    };
    drop(token_guard); // Release the lock

    // Attempt to log in and then search
    match search_bluesky_posts(&client, &token, &tags, limit_param, maybe_since_time).await {
        Ok(posts) => {
            if posts.is_empty() {
                body.push_str("<p>No posts found for those hashtags.</p>");
            } else {
                body.push_str("<h2>Recent Posts</h2>");

                // Render each post
                for post in &posts {
                    let post_text = post.record.text.as_deref().unwrap_or("<no text>");
                    let author_handle = post.author.as_ref().and_then(|a| a.handle.clone()).unwrap_or_default();
                    let rkey = post.uri.split('/').last().unwrap_or("");
                    let post_link = format!("https://bsky.app/profile/{}/post/{}", author_handle, rkey);
                    let author_link = format!("https://bsky.app/profile/{}", author_handle);
                    let created_at = post.record.created_at.as_deref().unwrap_or("<unknown date>");

                    body.push_str(&format!(
                        r#"<div class="post-container">
                             <p class="post-text"><a href="{}">{}</a></p>
                             <p class="post-author">
                               <a href="{}">{}</a>
                               &nbsp;&middot;&nbsp;
                               {}
                             </p>
                           </div>"#,
                        post_link, post_text, author_link, author_handle, created_at
                    ));
                }
            }
        }
        Err(e) => {
            body.push_str(&format!("<p>Error searching posts: {}</p>", e));
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
    let bsky_state = BskyState {
        token: Arc::new(Mutex::new(None)),
    };
    HttpServer::new(move || App::new().app_data(web::Data::new(bsky_state.clone())).service(index))
        .bind(("0.0.0.0", 8080))?
        .run()
        .await
}
