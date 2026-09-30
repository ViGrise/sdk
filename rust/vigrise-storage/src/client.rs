use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures_util::{Stream, StreamExt};
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use vigrise_auth::{
    sign_request, sign_request_with_payload_hash, CredentialsProvider, UNSIGNED_PAYLOAD,
};

use crate::error::{from_response, StorageError};
use crate::types::*;

/// Characters left as they are in a path segment or query value.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

fn encode(value: &str) -> String {
    utf8_percent_encode(value, UNRESERVED).to_string()
}

/// Object key as a URL path: each segment encoded, slashes kept.
fn key_path(key: &str) -> String {
    key.split('/').map(encode).collect::<Vec<_>>().join("/")
}

enum Payload {
    /// Signed with its SHA-256.
    Buffered(Bytes),
    /// Streamed and signed as `UNSIGNED-PAYLOAD`; `u64` is its length.
    Streamed(reqwest::Body, u64),
}

#[derive(Clone)]
pub struct StorageClient {
    endpoint: String,
    credentials: Arc<dyn CredentialsProvider>,
    http: reqwest::Client,
}

impl StorageClient {
    /// `endpoint` is the service's base URL, for example
    /// `https://storage.vigrise.com`.
    pub fn new(endpoint: impl Into<String>, credentials: Arc<dyn CredentialsProvider>) -> Self {
        Self {
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            credentials,
            http: reqwest::Client::new(),
        }
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        mut headers: HeaderMap,
        payload: Payload,
    ) -> Result<reqwest::Response, StorageError> {
        let url = format!("{}{path}", self.endpoint);
        let uri: http::Uri = url
            .parse()
            .map_err(|_| StorageError::Transport(format!("{url} is not a valid URL")))?;
        let credentials = self
            .credentials
            .credentials()
            .await
            .map_err(|e| StorageError::Signing(e.to_string()))?;
        let (signed, body, length) = match payload {
            Payload::Buffered(bytes) => (
                sign_request(
                    &method,
                    &uri,
                    &headers,
                    &bytes,
                    &credentials,
                    "storage",
                    Utc::now(),
                ),
                reqwest::Body::from(bytes.clone()),
                bytes.len() as u64,
            ),
            Payload::Streamed(body, length) => (
                sign_request_with_payload_hash(
                    &method,
                    &uri,
                    &headers,
                    UNSIGNED_PAYLOAD,
                    &credentials,
                    "storage",
                    Utc::now(),
                ),
                body,
                length,
            ),
        };
        signed
            .map_err(|e| StorageError::Signing(e.to_string()))?
            .apply(&mut headers)
            .map_err(|e| StorageError::Signing(e.to_string()))?;
        headers.insert(http::header::CONTENT_LENGTH, HeaderValue::from(length));
        let response = self
            .http
            .request(method, url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|e| StorageError::Transport(e.to_string()))?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let body = response.bytes().await.unwrap_or_default();
        if status == 401 || status == 403 {
            self.credentials.invalidate();
        }
        Err(from_response(status, &body))
    }

    async fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, StorageError> {
        let mut headers = HeaderMap::new();
        let payload = match body {
            Some(body) => {
                headers.insert(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                );
                Payload::Buffered(Bytes::from(body.to_string()))
            }
            None => Payload::Buffered(Bytes::new()),
        };
        let response = self.send(method, path, headers, payload).await?;
        response
            .json()
            .await
            .map_err(|e| StorageError::Transport(e.to_string()))
    }

    async fn empty(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<(), StorageError> {
        let mut headers = HeaderMap::new();
        let payload = match body {
            Some(body) => {
                headers.insert(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                );
                Payload::Buffered(Bytes::from(body.to_string()))
            }
            None => Payload::Buffered(Bytes::new()),
        };
        self.send(method, path, headers, payload).await?;
        Ok(())
    }

    fn object_headers(options: &PutOptions) -> Result<HeaderMap, StorageError> {
        let mut headers = HeaderMap::new();
        let value = |v: &str| {
            HeaderValue::from_str(v)
                .map_err(|_| StorageError::Signing(format!("{v} is not a header value")))
        };
        if let Some(content_type) = &options.content_type {
            headers.insert(http::header::CONTENT_TYPE, value(content_type)?);
        }
        for (name, v) in &options.metadata {
            let name = HeaderName::from_bytes(format!("x-vgr-meta-{name}").as_bytes())
                .map_err(|_| StorageError::Signing(format!("{name} is not a metadata name")))?;
            headers.insert(name, value(v)?);
        }
        if !options.tags.is_empty() {
            let tagging: Vec<String> = options
                .tags
                .iter()
                .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
                .collect();
            headers.insert("x-vgr-tagging", value(&tagging.join("&"))?);
        }
        Ok(headers)
    }

    pub async fn create_bucket(&self, name: &str) -> Result<Bucket, StorageError> {
        self.json(Method::POST, "/v1/buckets", Some(json!({ "name": name })))
            .await
    }

    pub async fn list_buckets(&self) -> Result<Vec<Bucket>, StorageError> {
        #[derive(Deserialize)]
        struct Listed {
            buckets: Vec<Bucket>,
        }
        let listed: Listed = self.json(Method::GET, "/v1/buckets", None).await?;
        Ok(listed.buckets)
    }

    pub async fn get_bucket(&self, name: &str) -> Result<Bucket, StorageError> {
        self.json(Method::GET, &format!("/v1/buckets/{name}"), None)
            .await
    }

    pub async fn delete_bucket(&self, name: &str) -> Result<(), StorageError> {
        self.empty(Method::DELETE, &format!("/v1/buckets/{name}"), None)
            .await
    }

    /// Uploads `body` in one request, signed with its hash.
    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: impl Into<Bytes>,
        options: &PutOptions,
    ) -> Result<PutResult, StorageError> {
        let response = self
            .send(
                Method::PUT,
                &format!("/v1/buckets/{bucket}/objects/{}", key_path(key)),
                Self::object_headers(options)?,
                Payload::Buffered(body.into()),
            )
            .await?;
        response
            .json()
            .await
            .map_err(|e| StorageError::Transport(e.to_string()))
    }

    /// Uploads `length` bytes from `body` in one streamed request.
    pub async fn put_object_stream<S>(
        &self,
        bucket: &str,
        key: &str,
        body: S,
        length: u64,
        options: &PutOptions,
    ) -> Result<PutResult, StorageError>
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    {
        let response = self
            .send(
                Method::PUT,
                &format!("/v1/buckets/{bucket}/objects/{}", key_path(key)),
                Self::object_headers(options)?,
                Payload::Streamed(reqwest::Body::wrap_stream(body), length),
            )
            .await?;
        response
            .json()
            .await
            .map_err(|e| StorageError::Transport(e.to_string()))
    }

    async fn read(
        &self,
        method: Method,
        bucket: &str,
        key: &str,
        range: Option<(u64, u64)>,
    ) -> Result<Object, StorageError> {
        let mut headers = HeaderMap::new();
        if let Some((first, last)) = range {
            headers.insert(
                http::header::RANGE,
                HeaderValue::from_str(&format!("bytes={first}-{last}")).expect("ASCII"),
            );
        }
        let response = self
            .send(
                method,
                &format!("/v1/buckets/{bucket}/objects/{}", key_path(key)),
                headers,
                Payload::Buffered(Bytes::new()),
            )
            .await?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let size = header("content-range")
            .and_then(|r| r.rsplit('/').next().and_then(|s| s.parse().ok()))
            .or_else(|| header("content-length").and_then(|l| l.parse().ok()));
        let content_type = header("content-type");
        let etag = header("etag").map(|e| e.trim_matches('"').to_string());
        let metadata: BTreeMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                let suffix = name.as_str().strip_prefix("x-vgr-meta-")?;
                Some((suffix.to_string(), value.to_str().ok()?.to_string()))
            })
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| StorageError::Transport(e.to_string()))?;
        Ok(Object {
            body,
            content_type,
            etag,
            size,
            metadata,
        })
    }

    /// Downloads an object, or the inclusive byte range `(first, last)` of it.
    pub async fn get_object(
        &self,
        bucket: &str,
        key: &str,
        range: Option<(u64, u64)>,
    ) -> Result<Object, StorageError> {
        self.read(Method::GET, bucket, key, range).await
    }

    /// An object's headers without its body.
    pub async fn head_object(&self, bucket: &str, key: &str) -> Result<Object, StorageError> {
        self.read(Method::HEAD, bucket, key, None).await
    }

    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        self.empty(
            Method::DELETE,
            &format!("/v1/buckets/{bucket}/objects/{}", key_path(key)),
            None,
        )
        .await
    }

    pub async fn list_objects(
        &self,
        bucket: &str,
        options: &ListOptions,
    ) -> Result<ListPage, StorageError> {
        let mut query = Vec::new();
        if let Some(prefix) = &options.prefix {
            query.push(format!("prefix={}", encode(prefix)));
        }
        if let Some(delimiter) = &options.delimiter {
            query.push(format!("delimiter={}", encode(delimiter)));
        }
        if let Some(max_keys) = options.max_keys {
            query.push(format!("max_keys={max_keys}"));
        }
        if let Some(token) = &options.continuation_token {
            query.push(format!("continuation_token={}", encode(token)));
        }
        if let Some(start_after) = &options.start_after {
            query.push(format!("start_after={}", encode(start_after)));
        }
        let path = if query.is_empty() {
            format!("/v1/buckets/{bucket}/objects")
        } else {
            format!("/v1/buckets/{bucket}/objects?{}", query.join("&"))
        };
        self.json(Method::GET, &path, None).await
    }

    /// Copies an object, in this or another bucket, without moving its bytes.
    pub async fn copy_object(
        &self,
        source_bucket: &str,
        source_key: &str,
        bucket: &str,
        key: &str,
        description: &CopyDescription,
    ) -> Result<PutResult, StorageError> {
        let mut headers = match description {
            CopyDescription::Copy => HeaderMap::new(),
            CopyDescription::Replace(options) => {
                let mut headers = Self::object_headers(options)?;
                headers.insert(
                    "x-vgr-metadata-directive",
                    HeaderValue::from_static("REPLACE"),
                );
                headers
            }
        };
        let source = format!("{source_bucket}/{}", key_path(source_key));
        headers.insert(
            "x-vgr-copy-source",
            HeaderValue::from_str(&source)
                .map_err(|_| StorageError::Signing(format!("{source} is not a header value")))?,
        );
        let response = self
            .send(
                Method::PUT,
                &format!("/v1/buckets/{bucket}/objects/{}", key_path(key)),
                headers,
                Payload::Buffered(Bytes::new()),
            )
            .await?;
        response
            .json()
            .await
            .map_err(|e| StorageError::Transport(e.to_string()))
    }

    /// Lists the multipart uploads in progress.
    pub async fn list_uploads(
        &self,
        bucket: &str,
        options: &ListUploadsOptions,
    ) -> Result<UploadsPage, StorageError> {
        let mut query = Vec::new();
        if let Some(prefix) = &options.prefix {
            query.push(format!("prefix={}", encode(prefix)));
        }
        if let Some(delimiter) = &options.delimiter {
            query.push(format!("delimiter={}", encode(delimiter)));
        }
        if let Some(max_uploads) = options.max_uploads {
            query.push(format!("max_uploads={max_uploads}"));
        }
        if let Some(marker) = &options.key_marker {
            query.push(format!("key_marker={}", encode(marker)));
        }
        if let Some(marker) = &options.upload_id_marker {
            query.push(format!("upload_id_marker={}", encode(marker)));
        }
        let path = if query.is_empty() {
            format!("/v1/buckets/{bucket}/uploads")
        } else {
            format!("/v1/buckets/{bucket}/uploads?{}", query.join("&"))
        };
        self.json(Method::GET, &path, None).await
    }

    /// Starts a multipart upload and returns its id.
    pub async fn create_upload(
        &self,
        bucket: &str,
        key: &str,
        options: &PutOptions,
    ) -> Result<String, StorageError> {
        #[derive(Deserialize)]
        struct Created {
            upload_id: String,
        }
        let created: Created = self
            .json(
                Method::POST,
                &format!("/v1/buckets/{bucket}/uploads"),
                Some(json!({
                  "key": key,
                  "content_type": options.content_type,
                  "metadata": options.metadata,
                  "tags": options.tags,
                })),
            )
            .await?;
        Ok(created.upload_id)
    }

    pub async fn upload_part(
        &self,
        bucket: &str,
        upload_id: &str,
        number: u32,
        body: impl Into<Bytes>,
    ) -> Result<CompletedPart, StorageError> {
        self.json_with_payload(
            Method::PUT,
            &format!("/v1/buckets/{bucket}/uploads/{upload_id}/parts/{number}"),
            Payload::Buffered(body.into()),
        )
        .await
    }

    async fn json_with_payload<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        payload: Payload,
    ) -> Result<T, StorageError> {
        let response = self.send(method, path, HeaderMap::new(), payload).await?;
        response
            .json()
            .await
            .map_err(|e| StorageError::Transport(e.to_string()))
    }

    pub async fn complete_upload(
        &self,
        bucket: &str,
        upload_id: &str,
        parts: &[CompletedPart],
    ) -> Result<PutResult, StorageError> {
        self.json(
            Method::POST,
            &format!("/v1/buckets/{bucket}/uploads/{upload_id}/complete"),
            Some(json!({ "parts": parts })),
        )
        .await
    }

    pub async fn abort_upload(&self, bucket: &str, upload_id: &str) -> Result<(), StorageError> {
        self.empty(
            Method::DELETE,
            &format!("/v1/buckets/{bucket}/uploads/{upload_id}"),
            None,
        )
        .await
    }

    /// Uploads a stream of any length in parts of `part_size` bytes (at least
    /// the service's smallest part, 5 MiB by default), aborting the upload when
    /// a part fails.
    pub async fn upload_stream<S>(
        &self,
        bucket: &str,
        key: &str,
        mut body: S,
        part_size: usize,
        options: &PutOptions,
    ) -> Result<PutResult, StorageError>
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Unpin,
    {
        let upload_id = self.create_upload(bucket, key, options).await?;
        let result = async {
            let mut parts = Vec::new();
            let mut buffer = BytesMut::new();
            let mut ended = false;
            while !ended {
                while buffer.len() < part_size {
                    match body.next().await {
                        Some(chunk) => buffer.extend_from_slice(
                            &chunk.map_err(|e| StorageError::Transport(e.to_string()))?,
                        ),
                        None => {
                            ended = true;
                            break;
                        }
                    }
                }
                if buffer.is_empty() && !parts.is_empty() {
                    break;
                }
                let take = buffer.len().min(part_size);
                let part = buffer.split_to(take).freeze();
                let number = parts.len() as u32 + 1;
                parts.push(self.upload_part(bucket, &upload_id, number, part).await?);
                if !buffer.is_empty() {
                    ended = false;
                }
            }
            self.complete_upload(bucket, &upload_id, &parts).await
        }
        .await;
        if result.is_err() {
            self.abort_upload(bucket, &upload_id).await.ok();
        }
        result
    }

    pub async fn put_tags(
        &self,
        bucket: &str,
        key: &str,
        tags: &BTreeMap<String, String>,
    ) -> Result<(), StorageError> {
        self.empty(
            Method::PUT,
            &format!("/v1/buckets/{bucket}/objects/{}?tagging", key_path(key)),
            Some(json!({ "tags": tags })),
        )
        .await
    }

    pub async fn get_tags(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<BTreeMap<String, String>, StorageError> {
        #[derive(Deserialize)]
        struct Tags {
            tags: BTreeMap<String, String>,
        }
        let tags: Tags = self
            .json(
                Method::GET,
                &format!("/v1/buckets/{bucket}/objects/{}?tagging", key_path(key)),
                None,
            )
            .await?;
        Ok(tags.tags)
    }

    pub async fn delete_tags(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        self.empty(
            Method::DELETE,
            &format!("/v1/buckets/{bucket}/objects/{}?tagging", key_path(key)),
            None,
        )
        .await
    }

    pub async fn presign(
        &self,
        bucket: &str,
        request: &PresignRequest,
    ) -> Result<Presigned, StorageError> {
        self.json(
            Method::POST,
            &format!("/v1/buckets/{bucket}/presign"),
            Some(serde_json::to_value(request).expect("presign request serializes")),
        )
        .await
    }

    pub async fn put_lifecycle(
        &self,
        bucket: &str,
        rules: &[LifecycleRule],
    ) -> Result<(), StorageError> {
        self.empty(
            Method::PUT,
            &format!("/v1/buckets/{bucket}/lifecycle"),
            Some(json!({ "rules": rules })),
        )
        .await
    }

    pub async fn get_lifecycle(&self, bucket: &str) -> Result<Vec<LifecycleRule>, StorageError> {
        #[derive(Deserialize)]
        struct Rules {
            rules: Vec<LifecycleRule>,
        }
        let rules: Rules = self
            .json(
                Method::GET,
                &format!("/v1/buckets/{bucket}/lifecycle"),
                None,
            )
            .await?;
        Ok(rules.rules)
    }

    pub async fn delete_lifecycle(&self, bucket: &str) -> Result<(), StorageError> {
        self.empty(
            Method::DELETE,
            &format!("/v1/buckets/{bucket}/lifecycle"),
            None,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_keep_their_slashes_and_encode_the_rest() {
        assert_eq!(
            key_path("photos/2026/ảnh 1.jpg"),
            "photos/2026/%E1%BA%A3nh%201.jpg"
        );
        assert_eq!(key_path("a+b?c#d"), "a%2Bb%3Fc%23d");
    }
}
