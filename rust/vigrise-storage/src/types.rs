use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Bucket {
    pub name: String,
    pub project_id: String,
    pub created_at: String,
}

/// Options of an upload.
#[derive(Debug, Clone, Default)]
pub struct PutOptions {
    pub content_type: Option<String>,
    /// User metadata, sent as `x-vgr-meta-<name>` headers.
    pub metadata: BTreeMap<String, String>,
    pub tags: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PutResult {
    pub etag: String,
    pub size: u64,
}

/// A downloaded object or byte range.
#[derive(Debug, Clone)]
pub struct Object {
    pub body: bytes::Bytes,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    /// Size of the whole object, also for a range.
    pub size: Option<u64>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_keys: Option<u32>,
    pub continuation_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ObjectSummary {
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub content_type: String,
    pub last_modified: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ListPage {
    pub objects: Vec<ObjectSummary>,
    pub common_prefixes: Vec<String>,
    pub next_continuation_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedPart {
    pub number: u32,
    pub etag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PresignRequest {
    pub key: String,
    /// `GET` or `PUT`.
    pub method: String,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Presigned {
    pub url: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleRule {
    pub id: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expire_after_days: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abort_incomplete_uploads_after_days: Option<u32>,
}
