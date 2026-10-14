//! Walks through the Speech API: a synthesis, a job that reports to a webhook,
//! the job's audio, and the verified webhook.
//!
//! `cargo run --example speech_tour --features speech -- <gateway url> <voice>`
//! The gateway must be able to reach `127.0.0.1`, where the example listens.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use tokio::sync::mpsc;
use vigrise::auth::Credentials;
use vigrise::speech::{
  JobInput, JobOptions, SpeechClient, SynthesizeOptions, WaitOptions, WebhookEvent, verify_webhook,
};

#[derive(Clone)]
struct Receiver {
  secret: Arc<String>,
  events: mpsc::UnboundedSender<WebhookEvent>,
}

async fn receive(State(receiver): State<Receiver>, headers: HeaderMap, body: Bytes) -> StatusCode {
  match verify_webhook(&receiver.secret, &headers, &body) {
    Ok(event) => {
      let _ = receiver.events.send(event);
      StatusCode::OK
    }
    Err(_) => StatusCode::BAD_REQUEST,
  }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut args = std::env::args().skip(1);
  let endpoint = args
    .next()
    .ok_or("usage: speech_tour <gateway url> <voice>")?;
  let voice = args
    .next()
    .ok_or("usage: speech_tour <gateway url> <voice>")?;
  let speech = SpeechClient::new(Arc::new(Credentials::resolve()?)).with_endpoint(endpoint);

  let spoken = speech
    .synthesize("Xin chào", &voice, &SynthesizeOptions::default())
    .await?;
  println!(
    "synthesized {} bytes as {}",
    spoken.audio.len(),
    spoken.generation_id
  );

  let secret = speech.rotate_webhook_secret().await?.secret;
  let (events, mut received) = mpsc::unbounded_channel();
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
  let hook = format!("http://{}/hook", listener.local_addr()?);
  let app = Router::new()
    .route("/hook", post(receive))
    .with_state(Receiver {
      secret: Arc::new(secret),
      events,
    });
  tokio::spawn(async move { axum::serve(listener, app).await });

  let job = speech
    .create_job(
      &JobInput::Text {
        text: "Xin chào các bạn".into(),
        voice,
      },
      &JobOptions {
        webhook_url: Some(hook),
        ..Default::default()
      },
    )
    .await?;
  let finished = speech
    .wait_for_job(
      &job.id,
      &WaitOptions {
        poll_interval: Duration::from_millis(500),
        timeout: Some(Duration::from_secs(60)),
      },
    )
    .await?;
  let generation = finished
    .generation_id
    .ok_or("the finished job has no generation")?;
  let audio = speech.download_generation_audio(&generation).await?;
  println!(
    "job {} {}: {} bytes of audio",
    finished.id,
    finished.status,
    audio.len()
  );

  let event = tokio::time::timeout(Duration::from_secs(60), received.recv())
    .await?
    .ok_or("the webhook listener stopped")?;
  println!("webhook {} for {}", event.kind, event.data.id);
  println!("tour ok");
  Ok(())
}
