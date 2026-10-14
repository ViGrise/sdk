use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use http::{HeaderMap, HeaderValue, Method};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use crate::auth::CredentialsProvider;
use crate::common::endpoint::resolve_endpoint;
use crate::common::multipart::Multipart;
use crate::common::pagination::Page;
use crate::common::signed::{Payload, SignedHttp};
use crate::speech::dto::*;
use crate::speech::error::{SpeechError, from_response};

/// Characters left as they are in a path segment or query value.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
  .remove(b'-')
  .remove(b'.')
  .remove(b'_')
  .remove(b'~');

fn encode(value: &str) -> String {
  utf8_percent_encode(value, UNRESERVED).to_string()
}

/// Appends `?name=value&...` for the pairs that have a value.
fn with_query(path: String, pairs: &[(&str, Option<String>)]) -> String {
  let query: Vec<String> = pairs
    .iter()
    .filter_map(|(name, value)| {
      value
        .as_ref()
        .map(|value| format!("{name}={}", encode(value)))
    })
    .collect();
  if query.is_empty() {
    path
  } else {
    format!("{path}?{}", query.join("&"))
  }
}

/// Audio sent while it is produced, with the metadata of the answer. Read the
/// chunks by polling it as a [`Stream`].
pub struct SpeechStream {
  /// Content type of the audio, such as `audio/pcm`.
  pub content_type: String,
  /// Id of the generation that holds the audio.
  pub generation_id: String,
  /// Samples per second, when the service reports it.
  pub sample_rate: Option<u32>,
  chunks: Pin<Box<dyn Stream<Item = Result<Bytes, SpeechError>> + Send>>,
}

impl Stream for SpeechStream {
  type Item = Result<Bytes, SpeechError>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.chunks.as_mut().poll_next(cx)
  }
}

/// A client for one Speech API endpoint. Every call is VGR4-signed for the
/// `speech` service. Clones share the credentials provider.
#[derive(Clone)]
pub struct SpeechClient {
  signed: SignedHttp,
}

impl SpeechClient {
  /// A client of the endpoint that `VIGRISE_ENDPOINT_URL_SPEECH` or
  /// `VIGRISE_ENDPOINT_URL` names, `https://speech-api.vigrise.com` by default.
  pub fn new(credentials: Arc<dyn CredentialsProvider>) -> Self {
    Self {
      signed: SignedHttp::new("speech", resolve_endpoint("speech", None), credentials),
    }
  }

  /// Points the client at another endpoint, for example `http://localhost:8080`.
  pub fn with_endpoint(mut self, endpoint: impl AsRef<str>) -> Self {
    self.signed = self.signed.with_endpoint(endpoint);
    self
  }

  async fn send(
    &self,
    method: Method,
    target: &str,
    mut headers: HeaderMap,
    body: Bytes,
    idempotency_key: Option<&str>,
  ) -> Result<reqwest::Response, SpeechError> {
    if let Some(key) = idempotency_key {
      headers.insert(
        "idempotency-key",
        HeaderValue::from_str(key)
          .map_err(|_| SpeechError::Signing("the idempotency key is not a header value".into()))?,
      );
    }
    Ok(
      self
        .signed
        .send(method, target, headers, Payload::Buffered(body))
        .await?,
    )
  }

  async fn json_request<T: DeserializeOwned>(
    &self,
    method: Method,
    target: &str,
    body: Option<Value>,
    idempotency_key: Option<&str>,
  ) -> Result<T, SpeechError> {
    let mut headers = HeaderMap::new();
    let payload = match body {
      Some(body) => {
        headers.insert(
          http::header::CONTENT_TYPE,
          HeaderValue::from_static("application/json"),
        );
        Bytes::from(body.to_string())
      }
      None => Bytes::new(),
    };
    let response = self
      .send(method, target, headers, payload, idempotency_key)
      .await?;
    response.json().await.map_err(SpeechError::from_transport)
  }

  async fn empty_request(&self, method: Method, target: &str) -> Result<(), SpeechError> {
    self
      .send(method, target, HeaderMap::new(), Bytes::new(), None)
      .await?;
    Ok(())
  }

  fn speech_body(text: &str, voice: &str, options: &SynthesizeOptions, stream: bool) -> Value {
    let mut body = Map::new();
    body.insert("input".into(), json!(text));
    body.insert("voice".into(), json!(voice));
    if let Some(model) = &options.model {
      body.insert("model".into(), json!(model));
    }
    if let Some(format) = &options.format {
      body.insert("response_format".into(), json!(format));
    }
    if let Some(store_audio) = options.store_audio {
      body.insert("store_audio".into(), json!(store_audio));
    }
    if stream {
      body.insert("stream".into(), json!(true));
    }
    Value::Object(body)
  }

  fn audio_metadata(response: &reqwest::Response) -> (String, String, Option<u32>) {
    let header = |name: &str| {
      response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
    };
    (
      header("content-type").unwrap_or_default().to_string(),
      header("x-vgr-generation-id")
        .unwrap_or_default()
        .to_string(),
      header("x-audio-sample-rate").and_then(|rate| rate.parse().ok()),
    )
  }

  /// Synthesizes `text` in `voice` and returns the whole audio.
  pub async fn synthesize(
    &self,
    text: &str,
    voice: &str,
    options: &SynthesizeOptions,
  ) -> Result<Speech, SpeechError> {
    let body = Self::speech_body(text, voice, options, false);
    let response = self
      .send_json(
        Method::POST,
        "/v1/audio/speech",
        body,
        options.idempotency_key.as_deref(),
      )
      .await?;
    let (content_type, generation_id, sample_rate) = Self::audio_metadata(&response);
    let audio = response
      .bytes()
      .await
      .map_err(SpeechError::from_transport)?;
    Ok(Speech {
      audio,
      content_type,
      generation_id,
      sample_rate,
    })
  }

  /// Synthesizes `text` in `voice` and returns the audio as it is produced.
  pub async fn synthesize_stream(
    &self,
    text: &str,
    voice: &str,
    options: &SynthesizeOptions,
  ) -> Result<SpeechStream, SpeechError> {
    let body = Self::speech_body(text, voice, options, true);
    let response = self
      .send_json(
        Method::POST,
        "/v1/audio/speech",
        body,
        options.idempotency_key.as_deref(),
      )
      .await?;
    let (content_type, generation_id, sample_rate) = Self::audio_metadata(&response);
    Ok(SpeechStream {
      content_type,
      generation_id,
      sample_rate,
      chunks: Box::pin(
        response
          .bytes_stream()
          .map(|chunk| chunk.map_err(SpeechError::from_transport)),
      ),
    })
  }

  async fn send_json(
    &self,
    method: Method,
    target: &str,
    body: Value,
    idempotency_key: Option<&str>,
  ) -> Result<reqwest::Response, SpeechError> {
    let mut headers = HeaderMap::new();
    headers.insert(
      http::header::CONTENT_TYPE,
      HeaderValue::from_static("application/json"),
    );
    self
      .send(
        method,
        target,
        headers,
        Bytes::from(body.to_string()),
        idempotency_key,
      )
      .await
  }

  /// The built-in voices of the project and its cloned voices.
  pub async fn list_voices(&self) -> Result<Vec<Voice>, SpeechError> {
    let page: Page<Voice> = self
      .json_request(Method::GET, "/v1/voices", None, None)
      .await?;
    Ok(page.data)
  }

  /// A cloned voice.
  pub async fn get_voice(&self, id: &str) -> Result<Voice, SpeechError> {
    self
      .json_request(
        Method::GET,
        &format!("/v1/voices/{}", encode(id)),
        None,
        None,
      )
      .await
  }

  /// Deletes a cloned voice.
  pub async fn delete_voice(&self, id: &str) -> Result<(), SpeechError> {
    self
      .empty_request(Method::DELETE, &format!("/v1/voices/{}", encode(id)))
      .await
  }

  /// Clones a voice from a recording of its speaker, who has given `consent`.
  pub async fn clone_voice(
    &self,
    name: &str,
    reference: &Reference,
    consent: &Consent,
    options: &CloneOptions,
  ) -> Result<Voice, SpeechError> {
    if reference.content_type.contains(['\r', '\n']) {
      return Err(SpeechError::Signing(
        "a content type cannot contain line breaks".into(),
      ));
    }
    let (content_type, body) = Multipart::new()
      .text("name", Bytes::copy_from_slice(name.as_bytes()))
      .file(
        "reference",
        &reference.file_name,
        &reference.content_type,
        reference.bytes.clone(),
      )
      .text(
        "consent",
        Bytes::from(serde_json::to_vec(consent).map_err(|e| SpeechError::Signing(e.to_string()))?),
      )
      .encode();
    let mut headers = HeaderMap::new();
    headers.insert(
      http::header::CONTENT_TYPE,
      HeaderValue::from_str(&content_type)
        .map_err(|_| SpeechError::Signing("the multipart boundary is not a header value".into()))?,
    );
    let response = self
      .send(
        Method::POST,
        "/v1/voices",
        headers,
        body,
        options.idempotency_key.as_deref(),
      )
      .await?;
    response.json().await.map_err(SpeechError::from_transport)
  }

  /// The statement a speaker agrees to before a voice is cloned.
  pub async fn consent_statement(&self) -> Result<ConsentStatement, SpeechError> {
    self
      .json_request(Method::GET, "/v1/consent-statement", None, None)
      .await
  }

  /// One page of the project's generations, newest first.
  pub async fn list_generations(
    &self,
    options: &ListGenerationsOptions,
  ) -> Result<Page<Generation>, SpeechError> {
    let target = with_query(
      "/v1/generations".into(),
      &[
        ("cursor", options.cursor.clone()),
        ("limit", options.limit.map(|limit| limit.to_string())),
        (
          "source",
          options.source.as_ref().map(|source| source.to_string()),
        ),
        ("voice", options.voice.clone()),
      ],
    );
    self.json_request(Method::GET, &target, None, None).await
  }

  /// A generation.
  pub async fn get_generation(&self, id: &str) -> Result<Generation, SpeechError> {
    self
      .json_request(
        Method::GET,
        &format!("/v1/generations/{}", encode(id)),
        None,
        None,
      )
      .await
  }

  /// Deletes a generation; with `all`, also the other generations of its job.
  pub async fn delete_generation(&self, id: &str, all: bool) -> Result<(), SpeechError> {
    let target = with_query(
      format!("/v1/generations/{}", encode(id)),
      &[("all", all.then(|| "true".to_string()))],
    );
    self.empty_request(Method::DELETE, &target).await
  }

  /// A temporary URL to download the audio of a generation from; with
  /// `download`, the URL makes browsers save the audio as a file.
  pub async fn generation_audio_url(
    &self,
    id: &str,
    download: bool,
  ) -> Result<AudioUrl, SpeechError> {
    let target = with_query(
      format!("/v1/generations/{}/audio", encode(id)),
      &[
        ("redirect", Some("false".to_string())),
        ("download", download.then(|| "true".to_string())),
      ],
    );
    self.json_request(Method::GET, &target, None, None).await
  }

  /// Downloads the audio of a generation.
  pub async fn download_generation_audio(&self, id: &str) -> Result<Bytes, SpeechError> {
    let location = self.generation_audio_url(id, false).await?;
    let response = self
      .signed
      .http()
      .get(&location.url)
      .send()
      .await
      .map_err(SpeechError::from_transport)?;
    if !response.status().is_success() {
      let status = response.status().as_u16();
      let body = response.bytes().await.unwrap_or_default();
      return Err(from_response(status, &body));
    }
    response.bytes().await.map_err(SpeechError::from_transport)
  }

  /// Starts a synthesis that runs in the background.
  pub async fn create_job(
    &self,
    input: &JobInput,
    options: &JobOptions,
  ) -> Result<Job, SpeechError> {
    let mut body = Map::new();
    match input {
      JobInput::Text { text, voice } => {
        body.insert("text".into(), json!(text));
        body.insert("voice".into(), json!(voice));
      }
      JobInput::Script { turns, gap_ms } => {
        body.insert("turns".into(), json!(turns));
        if let Some(gap_ms) = gap_ms {
          body.insert("gap_ms".into(), json!(gap_ms));
        }
      }
    }
    if let Some(model) = &options.model {
      body.insert("model".into(), json!(model));
    }
    if let Some(format) = &options.format {
      body.insert("response_format".into(), json!(format));
    }
    if let Some(webhook_url) = &options.webhook_url {
      body.insert("webhook_url".into(), json!(webhook_url));
    }
    if let Some(store_audio) = options.store_audio {
      body.insert("store_audio".into(), json!(store_audio));
    }
    self
      .json_request(
        Method::POST,
        "/v1/jobs",
        Some(Value::Object(body)),
        options.idempotency_key.as_deref(),
      )
      .await
  }

  /// A job.
  pub async fn get_job(&self, id: &str) -> Result<Job, SpeechError> {
    self
      .json_request(Method::GET, &format!("/v1/jobs/{}", encode(id)), None, None)
      .await
  }

  /// One page of the project's jobs, newest first.
  pub async fn list_jobs(&self, options: &ListJobsOptions) -> Result<Page<Job>, SpeechError> {
    let target = with_query(
      "/v1/jobs".into(),
      &[
        ("cursor", options.cursor.clone()),
        ("limit", options.limit.map(|limit| limit.to_string())),
        (
          "status",
          options.status.as_ref().map(|status| status.to_string()),
        ),
      ],
    );
    self.json_request(Method::GET, &target, None, None).await
  }

  /// Cancels a job that has not finished.
  pub async fn cancel_job(&self, id: &str) -> Result<Job, SpeechError> {
    self
      .json_request(
        Method::POST,
        &format!("/v1/jobs/{}/cancel", encode(id)),
        None,
        None,
      )
      .await
  }

  /// Polls a job until it succeeded, failed or was cancelled. On a timeout the
  /// error carries the job as it was at the last poll.
  pub async fn wait_for_job(&self, id: &str, options: &WaitOptions) -> Result<Job, SpeechError> {
    let started = Instant::now();
    loop {
      let job = self.get_job(id).await?;
      if job.status.is_finished() {
        return Ok(job);
      }
      if options
        .timeout
        .is_some_and(|timeout| started.elapsed() + options.poll_interval > timeout)
      {
        return Err(SpeechError::WaitTimeout {
          last_job: Box::new(job),
        });
      }
      tokio::time::sleep(options.poll_interval).await;
    }
  }

  /// The speech models of the engine.
  pub async fn list_models(&self) -> Result<Vec<Model>, SpeechError> {
    let page: Page<Model> = self
      .json_request(Method::GET, "/v1/models", None, None)
      .await?;
    Ok(page.data)
  }

  /// Creates the webhook secret of the project, or replaces the current one.
  pub async fn rotate_webhook_secret(&self) -> Result<WebhookSecret, SpeechError> {
    self
      .json_request(Method::POST, "/v1/webhook-secret", None, None)
      .await
  }
}
