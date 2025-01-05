use actix_web::{get, http::header, web, App, HttpResponse, HttpServer, Responder};
use dotenv::dotenv;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::env;

// The “record” part of each post
#[derive(Debug, Deserialize)]
struct BskyPostRecord {
    text: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

// A simplified representation of a single Bluesky post response
// from the hypothetical "searchPosts" xRPC. Adjust fields as needed:
#[derive(Debug, Deserialize)]
struct BskyPost {
    uri: String,
    #[serde(rename = "indexedAt")]
    indexed_at: String,
    record: BskyPostRecord,
}

// The top-level structure returned by Bluesky search endpoint:
#[derive(Debug, Deserialize)]
struct BskySearchPostsResponse {
    #[serde(default)]
    posts: Vec<BskyPost>,
}

// Session token response from Bluesky (com.atproto.server.createSession).
// Adjust fields as needed.
#[derive(Debug, Deserialize)]
struct BskySession {
    accessJwt: String, // The token we need for subsequent requests
}

/// Obtain a session token from Bluesky
async fn bluesky_login(client: &Client) -> Result<String, Box<dyn std::error::Error>> {
    // Load from environment
    let username = env::var("BLUESKY_USERNAME")?;
    let password = env::var("BLUESKY_PASSWORD")?;
    let base_url =
        env::var("BLUESKY_BASE_URL").unwrap_or_else(|_| "https://bsky.social".to_string());

    let url = format!("{}/xrpc/com.atproto.server.createSession", base_url);

    // Request a session token
    let resp = client
        .post(url)
        .json(&json!({
            "identifier": username,
            "password": password,
        }))
        .send()
        .await?
        .error_for_status()?;

    let session: BskySession = resp.json().await?;
    Ok(session.accessJwt)
}

/// Search Bluesky posts by hashtags (naive approach).
/// Adjust for the real Bluesky xRPC.
async fn search_bluesky_posts(
    client: &Client,
    token: &str,
    hashtags: &[String],
    max_posts: usize,
) -> Result<Vec<BskyPost>, Box<dyn std::error::Error>> {
    let base_url =
        env::var("BLUESKY_BASE_URL").unwrap_or_else(|_| "https://bsky.social".to_string());

    // The actual Bluesky xRPC for searching posts might require
    // a certain query param like `q=%23rust` for #rust.
    // Let's join them with spaces for a naive search approach: "#rust #actix"
    // Then URL-encode them.
    let joined_query = hashtags
        .iter()
        .map(|tag| format!("#{}", tag)) // prefix each with '#'
        .collect::<Vec<_>>()
        .join(" ");

    // We’ll limit to 50 in total, or user-specified. This is to keep
    // the search manageable. Some endpoints also have internal max limit.
    let limit = max_posts.min(50);

    // Hypothetical search endpoint is `app.bsky.feed.searchPosts`.
    // Adjust for the real one as needed. Example query param: `?q=%23rust+%23actix&limit=50`
    let url = format!("{}/xrpc/app.bsky.feed.searchPosts", base_url);

    let resp = client
        .get(url)
        .bearer_auth(token) // use session token
        .query(&[("q", &joined_query), ("limit", &limit.to_string())])
        .send()
        .await?
        .error_for_status()?;

    let text = resp.text().await?;
    println!("Raw response body: {text}");

    // Parse the JSON
    let mut result: BskySearchPostsResponse = serde_json::from_str(&text)?;
    result.posts.sort_by_key(|p| p.indexed_at.clone());
    result.posts.reverse(); // newest first
    Ok(result.posts)
}

#[get("/")]
async fn index(query: web::Query<HashMap<String, String>>) -> impl Responder {
    // The existing HTML content
    let main_body = r#"
        <p class="size-h1">Parameters:</p>
    "#;

    // Start with the main body content
    let mut body = String::from(main_body);

    // Append query parameters to the body
    for (key, value) in query.iter() {
        body.push_str(&format!("<p><strong>{}</strong>: {}</p>", key, value));
    }

    // Now parse out `tags` and `limit`
    // Using a separate struct is often cleaner, but let's do it inline:
    let tags_param = query.get("tags").cloned().unwrap_or_default();
    let limit_param = query
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10); // default to 10 if not present

    // Split comma-separated list of tags
    let tags: Vec<String> = tags_param
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .collect();

    // If no tags are provided, just return what we have so far
    if tags.is_empty() {
        body.push_str("<p>No tags specified, nothing to search for.</p>");
        return HttpResponse::Ok()
            .insert_header(("Widget-Title", "Test"))
            .insert_header(("Widget-Content-Type", "html"))
            .insert_header(header::ContentType::html())
            .body(body);
    }

    // Attempt to talk to Bluesky
    // In a real app, you'd store the client in AppState to reuse the same client.
    let client = Client::new();

    // We'll attempt to log in and search
    match bluesky_login(&client).await {
        Ok(token) => {
            // Now search
            match search_bluesky_posts(&client, &token, &tags, limit_param).await {
                Ok(posts) => {
                    if posts.is_empty() {
                        body.push_str("<p>No posts found for those hashtags.</p>");
                    } else {
                        body.push_str("<h2>Recent Posts</h2>");
                        for post in posts {
                            // Render a simple snippet in HTML
                            body.push_str(&format!(
                                "<div style=\"margin-bottom: 1em;\">
                                    <p>URI: {}</p>
                                    <p>Indexed At: {}</p>
                                    <p>Text: {}</p>
                                </div>",
                                post.uri, post.indexed_at, post.record.text
                            ));
                        }
                    }
                }
                Err(e) => {
                    body.push_str(&format!("<p>Error searching posts: {}</p>", e.to_string()));
                }
            }
        }
        Err(e) => {
            body.push_str(&format!(
                "<p>Error logging into Bluesky: {}</p>",
                e.to_string()
            ));
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
    // Load environment variables from .env
    dotenv().ok();

    HttpServer::new(|| App::new().service(index))
        .bind(("0.0.0.0", 8080))?
        .run()
        .await
}
