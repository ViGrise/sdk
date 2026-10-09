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
    /// Lowercase hex SHA-256 of the object, returned by `head_object` once the
    /// service knows it.
    pub checksum_sha256: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_keys: Option<u32>,
    pub continuation_token: Option<String>,
    /// A key after which the listing starts; ignored with a continuation token.
    pub start_after: Option<String>,
}

/// What a copy gives the new object.
#[derive(Debug, Clone, Default)]
pub enum CopyDescription {
    /// The new object keeps the source's content type, metadata, headers and tags.
    #[default]
    Copy,
    /// The new object takes its content type, metadata and tags from these options.
    Replace(PutOptions),
}

#[derive(Debug, Clone, Default)]
pub struct ListUploadsOptions {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_uploads: Option<u32>,
    pub key_marker: Option<String>,
    pub upload_id_marker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadSummary {
    pub upload_id: String,
    pub key: String,
    pub initiated: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadsPage {
    pub uploads: Vec<UploadSummary>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_key_marker: Option<String>,
    pub next_upload_id_marker: Option<String>,
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

/// A part stored so far in a multipart upload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadedPart {
    pub number: u32,
    pub etag: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedPart {
    pub number: u32,
    pub etag: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PresignRequest {
    pub key: String,
    /// `GET` or `PUT`.
    pub method: String,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
    /// The `Content-Disposition` a download's response states; part of the signature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_content_disposition: Option<String>,
    /// Lowercase hex SHA-256 an upload's content must have; the service refuses
    /// a mismatch with `BadDigest` and stores nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
    /// With `part_number`, a `PUT` stores that part of the multipart upload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_number: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Presigned {
    pub url: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// A disabled rule is stored and read back but never applied.
    #[serde(default = "enabled")]
    pub enabled: bool,
}

fn enabled() -> bool {
    true
}

impl Default for LifecycleRule {
    fn default() -> Self {
        Self {
            id: String::new(),
            prefix: String::new(),
            tags: BTreeMap::new(),
            expire_after_days: None,
            abort_incomplete_uploads_after_days: None,
            enabled: true,
        }
    }
}
