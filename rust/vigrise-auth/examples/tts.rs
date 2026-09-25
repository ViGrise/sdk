//! Calls the ViGrise TTS gateway with a VGR4 signature.
//!
//! `cargo run --example tts --features reqwest -- <gateway url> [voice]`
//! lists voices, or synthesizes "Xin chào" with `voice`. Credentials come from
//! the environment or `~/.vigrise/credentials`.

use vigrise_auth::reqwest_signer::sign_reqwest;
use vigrise_auth::Credentials;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut args = std::env::args().skip(1);
  let base = args.next().ok_or("usage: tts <gateway url> [voice]")?;
  let voice = args.next();
  let client = reqwest::Client::new();
  let mut request = match &voice {
    None => client.get(format!("{base}/v1/voices")).build()?,
    Some(voice) => client
      .post(format!("{base}/v1/audio/speech"))
      .json(&serde_json::json!({ "input": "Xin chào", "voice": voice, "response_format": "wav" }))
      .build()?,
  };
  sign_reqwest(&mut request, &Credentials::resolve()?, "tts", chrono::Utc::now())?;
  let response = client.execute(request).await?;
  let status = response.status();
  let body = response.bytes().await?;
  println!("{} {}", status.as_u16(), body.len());
  Ok(())
}
