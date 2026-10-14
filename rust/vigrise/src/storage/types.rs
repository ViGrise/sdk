use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A bucket.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Bucket {
  /// Name of the bucket.
  pub name: String,
  /// ID of the project that owns the bucket.
  pub project_id: String,
  /// Creation time, RFC 3339.
  pub created_at: String,
}

/// Options of an upload.
#[derive(Debug, Clone, Default)]
pub struct PutOptions {
  /// Content type stored with the object.
  pub content_type: Option<String>,
  /// User metadata, sent as `x-vgr-meta-<name>` headers.
  pub metadata: BTreeMap<String, String>,
  /// Tags of the object.
  pub tags: BTreeMap<String, String>,
}

/// The object a completed upload stored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PutResult {
  /// Entity tag of the object.
  pub etag: String,
  /// Size in bytes.
  pub size: u64,
}

/// A downloaded object or byte range.
#[derive(Debug, Clone)]
pub struct Object {
  /// The bytes of the object, or of the requested range.
  pub body: bytes::Bytes,
  /// Content type of the object.
  pub content_type: Option<String>,
  /// Entity tag of the object.
  pub etag: Option<String>,
  /// Size of the whole object, also for a range.
  pub size: Option<u64>,
  /// User metadata of the object.
  pub metadata: BTreeMap<String, String>,
  /// Lowercase hex SHA-256 of the object, returned by `head_object` once the
  /// service knows it.
  pub checksum_sha256: Option<String>,
}

/// Options of a listing of objects.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
  /// Only keys that start with this prefix.
  pub prefix: Option<String>,
  /// Groups keys that share a prefix up to the delimiter into common prefixes.
  pub delimiter: Option<String>,
  /// Most keys in one page.
  pub max_keys: Option<u32>,
  /// The token of the previous page.
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

/// Options of a listing of multipart uploads.
#[derive(Debug, Clone, Default)]
pub struct ListUploadsOptions {
  /// Only uploads of keys that start with this prefix.
  pub prefix: Option<String>,
  /// Groups keys that share a prefix up to the delimiter into common prefixes.
  pub delimiter: Option<String>,
  /// Most uploads in one page.
  pub max_uploads: Option<u32>,
  /// Key after which the listing starts.
  pub key_marker: Option<String>,
  /// Upload ID after which the listing starts, within `key_marker`.
  pub upload_id_marker: Option<String>,
}

/// A multipart upload in progress.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadSummary {
  /// ID of the upload.
  pub upload_id: String,
  /// Key of the object being uploaded.
  pub key: String,
  /// Start time, RFC 3339.
  pub initiated: String,
}

/// One page of multipart uploads in progress.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadsPage {
  /// The uploads of the page.
  pub uploads: Vec<UploadSummary>,
  /// Prefixes that group the keys the delimiter collapsed.
  pub common_prefixes: Vec<String>,
  /// Whether more uploads follow.
  pub is_truncated: bool,
  /// `key_marker` of the next page.
  pub next_key_marker: Option<String>,
  /// `upload_id_marker` of the next page.
  pub next_upload_id_marker: Option<String>,
}

/// An object in a listing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ObjectSummary {
  /// Key of the object.
  pub key: String,
  /// Size in bytes.
  pub size: u64,
  /// Entity tag of the object.
  pub etag: String,
  /// Content type of the object.
  pub content_type: String,
  /// Last modification time, RFC 3339.
  pub last_modified: String,
}

/// One page of objects.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ListPage {
  /// The objects of the page.
  pub objects: Vec<ObjectSummary>,
  /// Prefixes that group the keys the delimiter collapsed.
  pub common_prefixes: Vec<String>,
  /// Token of the next page; `None` on the last page.
  pub next_continuation_token: Option<String>,
}

/// A part stored so far in a multipart upload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadedPart {
  /// Number of the part.
  pub number: u32,
  /// Entity tag of the part.
  pub etag: String,
  /// Size in bytes.
  pub size: u64,
}

/// A part to complete a multipart upload with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedPart {
  /// Number of the part.
  pub number: u32,
  /// Entity tag the upload of the part returned.
  pub etag: String,
}

/// A request for a presigned URL.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PresignRequest {
  /// Key of the object.
  pub key: String,
  /// `GET` or `PUT`.
  pub method: String,
  /// Validity in seconds.
  pub expires_in: i64,
  /// Content type an upload must declare.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub content_type: Option<String>,
  /// Largest size in bytes an upload may have.
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
  /// Number of the part to store with `upload_id`.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub part_number: Option<u32>,
}

/// A presigned URL.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Presigned {
  /// The URL, usable without credentials until it expires.
  pub url: String,
  /// Expiry time, RFC 3339.
  pub expires_at: String,
}

/// A rule that expires objects or aborts stale uploads of a bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleRule {
  /// Name of the rule, unique within the bucket.
  pub id: String,
  /// Applies to keys that start with this prefix.
  #[serde(default)]
  pub prefix: String,
  /// Applies to objects that carry all these tags.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub tags: BTreeMap<String, String>,
  /// Deletes objects this many days after creation.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub expire_after_days: Option<u32>,
  /// Aborts multipart uploads still incomplete after this many days.
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
