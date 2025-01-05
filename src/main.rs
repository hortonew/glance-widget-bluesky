use actix_web::{get, http::header, web, App, HttpResponse, HttpServer, Responder};
use dotenv::dotenv;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::env;

/// Represents the *record* portion of a post, containing the
/// main text and the creation timestamp.
#[derive(Debug, Deserialize)]
struct BskyPostRecord {
    text: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

/// Represents a single Bluesky post from the xRPC response.
/// “indexedAt” is a top-level timestamp; “record” holds the text.
#[derive(Debug, Deserialize)]
struct BskyPost {
    uri: String,
    #[serde(rename = "indexedAt")]
    indexed_at: String,
    record: BskyPostRecord,
}

/// The top-level structure returned by the Bluesky “searchPosts” endpoint.
/// It contains an array of `BskyPost`s, and possibly a cursor for pagination.
#[derive(Debug, Deserialize)]
struct BskySearchPostsResponse {
    #[serde(default)]
    posts: Vec<BskyPost>,
}

/// Represents the session token retrieved from Bluesky login.
/// This “accessJwt” is required for subsequent authenticated requests.
#[derive(Debug, Deserialize)]
struct BskySession {
    accessJwt: String,
}

/// Logs in to Bluesky and returns a session token (JWT).
/// The username, password, and base URL are read from environment variables.
async fn bluesky_login(client: &Client) -> Result<String, Box<dyn std::error::Error>> {
    // Load credentials from environment
    let username = env::var("BLUESKY_USERNAME")?;
    let password = env::var("BLUESKY_PASSWORD")?;
    println!("Logging in as {}", username);
    let base_url =
        env::var("BLUESKY_BASE_URL").unwrap_or_else(|_| "https://bsky.social".to_string());

    // Construct the URL for session creation
    let url = format!("{}/xrpc/com.atproto.server.createSession", base_url);

    // Send the login request with JSON body
    let resp = client
        .post(url)
        .json(&json!({
            "identifier": username,
            "password": password,
        }))
        .send()
        .await?
        .error_for_status()?;

    // Deserialize the response to get the session token
    let session: BskySession = resp.json().await?;
    Ok(session.accessJwt)
}

/// Searches Bluesky posts using a naive hashtag-based query.
/// Joins the hashtags with spaces (“#tag1 #tag2”) and queries a hypothetical
/// endpoint “app.bsky.feed.searchPosts”. Returns a list of BskyPost.
async fn search_bluesky_posts(
    client: &Client,
    token: &str,
    hashtags: &[String],
    max_posts: usize,
) -> Result<Vec<BskyPost>, Box<dyn std::error::Error>> {
    let base_url =
        env::var("BLUESKY_BASE_URL").unwrap_or_else(|_| "https://bsky.social".to_string());

    // Construct the query by prefixing each hashtag with ‘#’
    // e.g. ["#rust", "#actix"] => "#rust #actix"
    let joined_query = hashtags
        .iter()
        .map(|tag| format!("#{}", tag))
        .collect::<Vec<_>>()
        .join(" ");

    // Limit is capped at 50 to avoid very large requests
    let limit = max_posts.min(50);

    // Call a hypothetical search endpoint: `app.bsky.feed.searchPosts`
    let url = format!("{}/xrpc/app.bsky.feed.searchPosts", base_url);

    // Perform the GET request, providing the token via Bearer authorization
    let resp = client
        .get(url)
        .bearer_auth(token)
        .query(&[("q", &joined_query), ("limit", &limit.to_string())])
        .send()
        .await?
        .error_for_status()?;

    // For debugging, print out the raw JSON response
    let text = resp.text().await?;
    // println!("Raw response body: {text}");

    // Deserialize the JSON into our BskySearchPostsResponse
    let mut result: BskySearchPostsResponse = serde_json::from_str(&text)?;

    // Sort posts by “indexed_at” descending (newest first)
    result.posts.sort_by_key(|p| p.indexed_at.clone());
    result.posts.reverse();

    // Return the sorted posts
    Ok(result.posts)
}

/// Handles GET requests on `/`.
/// Reads query parameters (?tags=..., ?limit=...), queries Bluesky, and
/// renders the resulting posts in HTML format.
#[get("/")]
async fn index(query: web::Query<HashMap<String, String>>) -> impl Responder {
    // Basic HTML skeleton
    let main_body = r#"
        <p class="size-h1">Parameters:</p>
    "#;
    let mut body = String::from(main_body);

    // Show all query parameters in the HTML
    for (key, value) in query.iter() {
        body.push_str(&format!("<p><strong>{}</strong>: {}</p>", key, value));
    }

    // Extract tags and limit from query parameters
    let tags_param = query.get("tags").cloned().unwrap_or_default();
    let limit_param = query
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10);

    // Split the tags by comma: e.g. "rust,actix" => ["rust", "actix"]
    let tags: Vec<String> = tags_param
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .collect();

    // If no tags are provided, display a short note
    if tags.is_empty() {
        body.push_str("<p>No tags specified, nothing to search for.</p>");
        return HttpResponse::Ok()
            .insert_header(("Widget-Title", "Test"))
            .insert_header(("Widget-Content-Type", "html"))
            .insert_header(header::ContentType::html())
            .body(body);
    }

    // Create a new HTTP client (in production, reuse or store it in App State)
    let client = Client::new();

    // Attempt to log in and retrieve a token
    match bluesky_login(&client).await {
        Ok(token) => {
            // With a valid token, search for posts
            match search_bluesky_posts(&client, &token, &tags, limit_param).await {
                Ok(posts) => {
                    // If no posts were found, inform the user
                    if posts.is_empty() {
                        body.push_str("<p>No posts found for those hashtags.</p>");
                    } else {
                        // Otherwise, list them
                        body.push_str("<h2>Recent Posts</h2>");
                        for post in posts {
                            body.push_str(&format!(
                                "<div style=\"margin-bottom: 1em;\">
                                    <p><strong>URI:</strong> {}</p>
                                    <p><strong>Indexed At:</strong> {}</p>
                                    <p><strong>Text:</strong> {}</p>
                                </div>",
                                post.uri, post.indexed_at, post.record.text
                            ));
                        }
                    }
                }
                Err(e) => {
                    // Handle any search errors
                    body.push_str(&format!("<p>Error searching posts: {}</p>", e.to_string()));
                }
            }
        }
        Err(e) => {
            // Handle login errors
            body.push_str(&format!(
                "<p>Error logging into Bluesky: {}</p>",
                e.to_string()
            ));
        }
    }

    // Return the final HTML
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
