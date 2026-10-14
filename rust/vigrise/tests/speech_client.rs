#![cfg(feature = "speech")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::Router;
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::{json, Value};
use vigrise::auth::Credentials;
use vigrise::speech::{
  AudioFormat, CloneOptions, Consent, GenerationSource, JobInput, JobOptions, JobStatus,
  ListGenerationsOptions, ListJobsOptions, Reference, Relationship, SpeechClient, SpeechError,
  SynthesizeOptions, Turn, VoiceKind, WaitOptions,
};

#[derive(Clone, Debug)]
struct Seen {
  method: String,
  target: String,
  headers: HeaderMap,
  body: Vec<u8>,
}

impl Seen {
  fn json(&self) -> Value {
    serde_json::from_slice(&self.body).unwrap()
  }

  fn header(&self, name: &str) -> Option<&str> {
    self.headers.get(name).and_then(|value| value.to_str().ok())
  }
}

type Responder = Arc<dyn Fn(&Seen) -> Response + Send + Sync>;

#[derive(Clone)]
struct State_ {
  seen: Arc<Mutex<Vec<Seen>>>,
  respond: Responder,
}

struct Fake {
  url: String,
  seen: Arc<Mutex<Vec<Seen>>>,
}

impl Fake {
  fn last(&self) -> Seen {
    self
      .seen
      .lock()
      .unwrap()
      .last()
      .cloned()
      .expect("a request")
  }

  fn count(&self) -> usize {
    self.seen.lock().unwrap().len()
  }
}

async fn handle(State(state): State<State_>, request: Request) -> Response {
  let (parts, body) = request.into_parts();
  let body = axum::body::to_bytes(body, usize::MAX).await.unwrap();
  let seen = Seen {
    method: parts.method.to_string(),
    target: parts
      .uri
      .path_and_query()
      .map(|target| target.to_string())
      .unwrap_or_default(),
    headers: parts.headers,
    body: body.to_vec(),
  };
  let response = (state.respond)(&seen);
  state.seen.lock().unwrap().push(seen);
  response
}

async fn fake(respond: impl Fn(&Seen) -> Response + Send + Sync + 'static) -> Fake {
  let state = State_ {
    seen: Arc::default(),
    respond: Arc::new(respond),
  };
  let seen = state.seen.clone();
  let app = Router::new().fallback(handle).with_state(state);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("http://{}", listener.local_addr().unwrap());
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  Fake { url, seen }
}

fn json_response(status: u16, body: Value) -> Response {
  Response::builder()
    .status(status)
    .header("content-type", "application/json")
    .body(Body::from(body.to_string()))
    .unwrap()
}

fn client(fake: &Fake) -> SpeechClient {
  SpeechClient::new(Arc::new(Credentials::new("VGAKTEST", "secret"))).with_endpoint(&fake.url)
}

fn job(status: &str) -> Value {
  json!({
    "id": "job_1", "object": "job", "status": status,
    "progress": {"pieces_done": 1, "pieces_total": 2, "characters_done": 5, "characters": 10},
    "characters_used": 5, "format": "wav", "webhook_url": null, "error": null,
    "generation_id": null, "created_by": "me", "created_at": "2026-10-01T12:00:00Z",
    "finished_at": null
  })
}

fn generation() -> Value {
  json!({
    "id": "gen_1", "object": "generation", "source": "sync", "job_id": null,
    "voice": "haley-f1", "model": "fp16", "format": "wav", "text": "Xin", "characters": 3,
    "characters_used": 3, "duration_ms": 900, "status": "complete",
    "audio": {"available": true, "bytes": 44, "expires_at": "2026-10-02T12:00:00Z"},
    "created_by": "me", "created_at": "2026-10-01T12:00:00Z"
  })
}

#[tokio::test]
async fn every_call_is_signed_vgr4_for_speech() {
  let fake = fake(|_| json_response(200, json!({"object": "list", "data": []}))).await;
  client(&fake).list_voices().await.unwrap();
  let seen = fake.last();
  let authorization = seen.header("authorization").unwrap();
  assert!(
    authorization.starts_with("VGR4-HMAC-SHA256 Credential=VGAKTEST/"),
    "{authorization}"
  );
  assert!(
    authorization.contains("/speech/vgr4_request"),
    "{authorization}"
  );
  assert!(seen.header("x-vgr-date").is_some());
  assert!(seen.header("x-vgr-content-sha256").is_some());
}

#[tokio::test]
async fn synthesize_posts_the_request_and_reads_the_audio_headers() {
  let fake = fake(|_| {
    Response::builder()
      .header("content-type", "audio/wav")
      .header("x-vgr-generation-id", "gen_9")
      .header("x-audio-sample-rate", "24000")
      .body(Body::from(Bytes::from_static(b"RIFFaudio")))
      .unwrap()
  })
  .await;
  let speech = client(&fake)
    .synthesize(
      "Xin chào",
      "haley-f1",
      &SynthesizeOptions {
        model: Some("fp16".into()),
        format: Some(AudioFormat::Mp3),
        store_audio: Some(false),
        idempotency_key: Some("key-1".into()),
      },
    )
    .await
    .unwrap();
  assert_eq!(&speech.audio[..], b"RIFFaudio");
  assert_eq!(speech.content_type, "audio/wav");
  assert_eq!(speech.generation_id, "gen_9");
  assert_eq!(speech.sample_rate, Some(24000));
  let seen = fake.last();
  assert_eq!(
    (seen.method.as_str(), seen.target.as_str()),
    ("POST", "/v1/audio/speech")
  );
  assert_eq!(seen.header("idempotency-key"), Some("key-1"));
  assert_eq!(seen.header("content-type"), Some("application/json"));
  assert_eq!(
    seen.json(),
    json!({"input": "Xin chào", "voice": "haley-f1", "model": "fp16", "response_format": "mp3", "store_audio": false})
  );
}

#[tokio::test]
async fn synthesize_leaves_out_what_is_not_set() {
  let fake = fake(|_| {
    Response::builder()
      .header("content-type", "audio/wav")
      .header("x-vgr-generation-id", "gen_9")
      .body(Body::from("a"))
      .unwrap()
  })
  .await;
  let speech = client(&fake)
    .synthesize("hi", "haley-f1", &SynthesizeOptions::default())
    .await
    .unwrap();
  assert_eq!(speech.sample_rate, None);
  let seen = fake.last();
  assert_eq!(seen.json(), json!({"input": "hi", "voice": "haley-f1"}));
  assert!(seen.header("idempotency-key").is_none());
}

#[tokio::test]
async fn synthesize_stream_yields_the_chunks_and_the_metadata() {
  let fake = fake(|_| {
    let chunks = futures_util::stream::iter(vec![
      Ok::<_, std::io::Error>(Bytes::from_static(b"abc")),
      Ok(Bytes::from_static(b"def")),
    ]);
    Response::builder()
      .header("content-type", "audio/pcm")
      .header("x-vgr-generation-id", "gen_s")
      .header("x-audio-sample-rate", "16000")
      .body(Body::from_stream(chunks))
      .unwrap()
  })
  .await;
  let mut stream = client(&fake)
    .synthesize_stream("hi", "haley-f1", &SynthesizeOptions::default())
    .await
    .unwrap();
  assert_eq!(stream.generation_id, "gen_s");
  assert_eq!(stream.content_type, "audio/pcm");
  assert_eq!(stream.sample_rate, Some(16000));
  let mut audio = Vec::new();
  while let Some(chunk) = stream.next().await {
    audio.extend_from_slice(&chunk.unwrap());
  }
  assert_eq!(audio, b"abcdef");
  assert_eq!(fake.last().json()["stream"], true);
}

#[tokio::test]
async fn voices_are_listed_read_and_deleted() {
  let fake = fake(|seen| match (seen.method.as_str(), seen.target.as_str()) {
    ("GET", "/v1/voices") => json_response(
      200,
      json!({"object": "list", "data": [
        {"id": "haley-f1", "object": "voice", "type": "system"},
        {"id": "voice_1", "object": "voice", "type": "cloned", "name": "Mine", "duration_ms": 8000,
         "created_by": "me", "created_at": "2026-10-01T12:00:00Z"}
      ]}),
    ),
    ("GET", "/v1/voices/voice_1") => json_response(
      200,
      json!({"id": "voice_1", "object": "voice", "type": "cloned", "name": "Mine", "duration_ms": 8000,
             "created_by": "me", "created_at": "2026-10-01T12:00:00Z"}),
    ),
    ("DELETE", "/v1/voices/voice_1") => Response::builder().status(204).body(Body::empty()).unwrap(),
    _ => json_response(404, json!({"error": {"code": "NotFound", "message": "no"}})),
  })
  .await;
  let client = client(&fake);
  let voices = client.list_voices().await.unwrap();
  assert_eq!(voices.len(), 2);
  assert_eq!(voices[0].kind, VoiceKind::System);
  assert_eq!(voices[1].kind, VoiceKind::Cloned);
  assert_eq!(voices[1].name.as_deref(), Some("Mine"));
  assert_eq!(
    client.get_voice("voice_1").await.unwrap().duration_ms,
    Some(8000)
  );
  client.delete_voice("voice_1").await.unwrap();
  assert_eq!(fake.last().method, "DELETE");
}

#[tokio::test]
async fn clone_voice_sends_name_reference_and_consent_as_multipart() {
  let fake = fake(|_| {
    json_response(
      201,
      json!({"id": "voice_2", "object": "voice", "type": "cloned", "name": "Mine", "duration_ms": 8000,
             "created_by": "me", "created_at": "2026-10-01T12:00:00Z"}),
    )
  })
  .await;
  let voice = client(&fake)
    .clone_voice(
      "Mine",
      &Reference::new(&b"RIFF\x00\x01"[..], "audio/wav", "me.wav"),
      &Consent {
        statement_version: 1,
        speaker_name: "An".into(),
        relationship: Relationship::Own,
        agreed: true,
      },
      &CloneOptions {
        idempotency_key: Some("clone-1".into()),
      },
    )
    .await
    .unwrap();
  assert_eq!(voice.id, "voice_2");
  let seen = fake.last();
  assert_eq!(
    (seen.method.as_str(), seen.target.as_str()),
    ("POST", "/v1/voices")
  );
  assert_eq!(seen.header("idempotency-key"), Some("clone-1"));
  let content_type = seen.header("content-type").unwrap();
  let boundary = content_type
    .strip_prefix("multipart/form-data; boundary=")
    .expect(content_type);
  let body = String::from_utf8_lossy(&seen.body).into_owned();
  assert!(body.starts_with(&format!("--{boundary}\r\n")), "{body}");
  assert!(body.ends_with(&format!("--{boundary}--\r\n")), "{body}");
  assert!(
    body.contains("Content-Disposition: form-data; name=\"name\"\r\n\r\nMine\r\n"),
    "{body}"
  );
  assert!(
    body.contains(
      "Content-Disposition: form-data; name=\"reference\"; filename=\"me.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFF\u{0}\u{1}\r\n"
    ),
    "{body}"
  );
  let consent_start = body.find("name=\"consent\"").expect("a consent part");
  let json_start = body[consent_start..].find("\r\n\r\n").unwrap() + consent_start + 4;
  let json_end = body[json_start..].find("\r\n--").unwrap() + json_start;
  let consent: Value = serde_json::from_str(&body[json_start..json_end]).unwrap();
  assert_eq!(
    consent,
    json!({"statement_version": 1, "speaker_name": "An", "relationship": "self", "agreed": true})
  );
}

#[tokio::test]
async fn the_consent_statement_is_read() {
  let fake = fake(|_| json_response(200, json!({"version": 3, "statement": "I agree"}))).await;
  let statement = client(&fake).consent_statement().await.unwrap();
  assert_eq!(
    (statement.version, statement.statement.as_str()),
    (3, "I agree")
  );
  assert_eq!(fake.last().target, "/v1/consent-statement");
}

#[tokio::test]
async fn generations_are_listed_read_and_deleted() {
  let fake = fake(|seen| match seen.method.as_str() {
    "DELETE" => Response::builder().status(204).body(Body::empty()).unwrap(),
    _ if seen.target.starts_with("/v1/generations?") => json_response(
      200,
      json!({"object": "list", "data": [generation()], "next_cursor": "c2"}),
    ),
    _ => json_response(200, generation()),
  })
  .await;
  let client = client(&fake);
  let page = client
    .list_generations(&ListGenerationsOptions {
      cursor: Some("c1".into()),
      limit: Some(5),
      source: Some(GenerationSource::Job),
      voice: Some("haley f1".into()),
    })
    .await
    .unwrap();
  assert_eq!(page.next_cursor.as_deref(), Some("c2"));
  assert_eq!(page.data[0].source, GenerationSource::Sync);
  assert_eq!(
    fake.last().target,
    "/v1/generations?cursor=c1&limit=5&source=job&voice=haley%20f1"
  );
  assert_eq!(
    client.get_generation("gen_1").await.unwrap().audio.bytes,
    44
  );
  client.delete_generation("gen_1", false).await.unwrap();
  assert_eq!(fake.last().target, "/v1/generations/gen_1");
  client.delete_generation("gen_1", true).await.unwrap();
  assert_eq!(fake.last().target, "/v1/generations/gen_1?all=true");
}

#[tokio::test]
async fn generation_audio_is_fetched_through_its_url_without_a_signature() {
  let fake = fake(|seen| {
    if seen.target.starts_with("/v1/generations/gen_1/audio") {
      let host = seen.header("host").unwrap();
      json_response(
        200,
        json!({"url": format!("http://{host}/blob/gen_1.wav"), "expires_at": "2026-10-01T13:00:00Z"}),
      )
    } else {
      Response::builder().body(Body::from("RIFFaudio")).unwrap()
    }
  })
  .await;
  let client = client(&fake);
  let url = client.generation_audio_url("gen_1", false).await.unwrap();
  assert_eq!(
    fake.last().target,
    "/v1/generations/gen_1/audio?redirect=false"
  );
  assert!(url.url.ends_with("/blob/gen_1.wav"));
  client.generation_audio_url("gen_1", true).await.unwrap();
  assert_eq!(
    fake.last().target,
    "/v1/generations/gen_1/audio?redirect=false&download=true"
  );
  let audio = client.download_generation_audio("gen_1").await.unwrap();
  assert_eq!(&audio[..], b"RIFFaudio");
  let download = fake.last();
  assert_eq!(download.target, "/blob/gen_1.wav");
  assert!(download.header("authorization").is_none());
}

#[tokio::test]
async fn jobs_are_created_from_text_or_a_script() {
  let fake = fake(|_| json_response(202, job("queued"))).await;
  let client = client(&fake);
  let created = client
    .create_job(
      &JobInput::Text {
        text: "Xin".into(),
        voice: "haley-f1".into(),
      },
      &JobOptions {
        model: Some("fp16".into()),
        format: Some(AudioFormat::Opus),
        webhook_url: Some("https://example.com/hook".into()),
        store_audio: Some(true),
        idempotency_key: Some("job-1".into()),
      },
    )
    .await
    .unwrap();
  assert_eq!(created.status, JobStatus::Queued);
  let seen = fake.last();
  assert_eq!(
    (seen.method.as_str(), seen.target.as_str()),
    ("POST", "/v1/jobs")
  );
  assert_eq!(seen.header("idempotency-key"), Some("job-1"));
  assert_eq!(
    seen.json(),
    json!({"text": "Xin", "voice": "haley-f1", "model": "fp16", "response_format": "opus",
           "webhook_url": "https://example.com/hook", "store_audio": true})
  );
  client
    .create_job(
      &JobInput::Script {
        turns: vec![
          Turn {
            voice: "a".into(),
            text: "one".into(),
          },
          Turn {
            voice: "b".into(),
            text: "two".into(),
          },
        ],
        gap_ms: Some(250),
      },
      &JobOptions::default(),
    )
    .await
    .unwrap();
  assert_eq!(
    fake.last().json(),
    json!({"turns": [{"voice": "a", "text": "one"}, {"voice": "b", "text": "two"}], "gap_ms": 250})
  );
}

#[tokio::test]
async fn jobs_are_read_listed_and_cancelled() {
  let fake = fake(|seen| {
    if seen.target.starts_with("/v1/jobs?") {
      json_response(
        200,
        json!({"object": "list", "data": [job("running")], "next_cursor": null}),
      )
    } else {
      json_response(200, job("cancelled"))
    }
  })
  .await;
  let client = client(&fake);
  assert_eq!(client.get_job("job_1").await.unwrap().id, "job_1");
  assert_eq!(fake.last().target, "/v1/jobs/job_1");
  let page = client
    .list_jobs(&ListJobsOptions {
      status: Some(JobStatus::Running),
      limit: Some(2),
      ..Default::default()
    })
    .await
    .unwrap();
  assert_eq!(page.data[0].status, JobStatus::Running);
  assert_eq!(page.next_cursor, None);
  assert_eq!(fake.last().target, "/v1/jobs?limit=2&status=running");
  let cancelled = client.cancel_job("job_1").await.unwrap();
  assert_eq!(cancelled.status, JobStatus::Cancelled);
  let seen = fake.last();
  assert_eq!(
    (seen.method.as_str(), seen.target.as_str()),
    ("POST", "/v1/jobs/job_1/cancel")
  );
}

#[tokio::test]
async fn an_unknown_status_is_kept_instead_of_failing() {
  let fake = fake(|_| json_response(200, job("paused"))).await;
  let job = client(&fake).get_job("job_1").await.unwrap();
  assert_eq!(job.status, JobStatus::Other("paused".into()));
  assert!(!job.status.is_finished());
}

#[tokio::test]
async fn wait_for_job_polls_until_a_final_state() {
  let polls = Arc::new(Mutex::new(0));
  let counter = polls.clone();
  let fake = fake(move |_| {
    let mut polls = counter.lock().unwrap();
    *polls += 1;
    json_response(
      200,
      job(["queued", "running", "succeeded", "failed"][(*polls - 1).min(3)]),
    )
  })
  .await;
  let finished = client(&fake)
    .wait_for_job(
      "job_1",
      &WaitOptions {
        poll_interval: Duration::from_millis(5),
        timeout: Some(Duration::from_secs(5)),
      },
    )
    .await
    .unwrap();
  assert_eq!(finished.status, JobStatus::Succeeded);
  assert_eq!(fake.count(), 3);
}

#[tokio::test]
async fn wait_for_job_times_out_with_the_last_job() {
  let fake = fake(|_| json_response(200, job("running"))).await;
  let error = client(&fake)
    .wait_for_job(
      "job_1",
      &WaitOptions {
        poll_interval: Duration::from_millis(5),
        timeout: Some(Duration::from_millis(40)),
      },
    )
    .await
    .unwrap_err();
  match error {
    SpeechError::WaitTimeout { last_job } => assert_eq!(last_job.status, JobStatus::Running),
    other => panic!("{other:?}"),
  }
}

#[tokio::test]
async fn models_and_the_webhook_secret() {
  let fake = fake(|seen| {
    if seen.target == "/v1/models" {
      json_response(
        200,
        json!({"object": "list", "data": [{"id": "fp16", "profile": "fast"}]}),
      )
    } else {
      json_response(
        200,
        json!({"secret": "whsec_abc", "created_at": "2026-10-01T12:00:00Z"}),
      )
    }
  })
  .await;
  let client = client(&fake);
  let models = client.list_models().await.unwrap();
  assert_eq!(models[0].id, "fp16");
  assert_eq!(models[0].extra["profile"], "fast");
  let secret = client.rotate_webhook_secret().await.unwrap();
  assert_eq!(secret.secret, "whsec_abc");
  let seen = fake.last();
  assert_eq!(
    (seen.method.as_str(), seen.target.as_str()),
    ("POST", "/v1/webhook-secret")
  );
}

#[tokio::test]
async fn error_envelopes_become_service_errors_with_their_extra_fields() {
  let fake = fake(|_| {
    json_response(
      409,
      json!({"error": {"code": "IdempotencyReplayed", "message": "already ran", "resource_id": "gen_5"}}),
    )
  })
  .await;
  let error = client(&fake)
    .synthesize("hi", "haley-f1", &SynthesizeOptions::default())
    .await
    .unwrap_err();
  match error {
    SpeechError::Service {
      status,
      code,
      resource_id,
      ..
    } => {
      assert_eq!((status, code.as_str()), (409, "IdempotencyReplayed"));
      assert_eq!(resource_id.as_deref(), Some("gen_5"));
    }
    other => panic!("{other:?}"),
  }
}

#[tokio::test]
async fn an_unreachable_endpoint_is_a_transport_error() {
  let client = SpeechClient::new(Arc::new(Credentials::new("VGAKTEST", "secret")))
    .with_endpoint("http://127.0.0.1:1");
  let error = client.list_voices().await.unwrap_err();
  assert!(matches!(error, SpeechError::Transport(_)), "{error:?}");
}
