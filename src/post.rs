use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// A single post from "app.bsky.feed.searchPosts".
/// We capture common fields plus a generic `extra` map for anything unknown.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct BskyPost {
    // example: https://jsonblob.com/1326024085142167552
    pub uri: String,
    cid: Option<String>,
    #[serde(rename = "indexedAt")]
    pub indexed_at: String,
    pub author: Option<BskyAuthor>,
    pub record: BskyPostRecord, // the actual text, timestamps, etc.
    #[serde(rename = "repostCount")]
    pub repost_count: Option<u32>,
    #[serde(rename = "replyCount")]
    pub reply_count: Option<u32>,
    #[serde(rename = "likeCount")]
    pub like_count: Option<u32>,
    #[serde(rename = "quoteCount")]
    pub quote_count: Option<u32>,

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
pub struct BskyAuthor {
    did: Option<String>,
    pub handle: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    avatar: Option<String>,

    #[serde(default)]
    associated: Value,
    #[serde(default)]
    labels: Value,

    // Flatten anything else
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

/// The “record” part of each post (contains the main text, facets, etc.).
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct BskyPostRecord {
    /// This is often present in Bluesky objects:
    #[serde(rename = "$type")]
    pub record_type: Option<String>,

    pub text: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,

    #[serde(default)]
    embed: Value,
    #[serde(default)]
    facets: Value,
    #[serde(default)]
    langs: Value,
    #[serde(default)]
    reply: Value,

    // Flatten anything else (like "text", "createdAt", etc. we didn't define)
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

/// The top-level structure for the "searchPosts" response
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct BskySearchPostsResponse {
    #[serde(default)]
    pub posts: Vec<BskyPost>,

    /// For pagination, if present
    #[serde(default)]
    cursor: Option<String>,

    #[serde(default)]
    sort: Option<String>,

    // Flatten anything else
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}
