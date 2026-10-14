//! Lists the voices, or synthesizes "Xin chào", through the Speech API.
//!
//! `cargo run --example speech --features speech -- <gateway url> [voice]`
//! prints the HTTP status, then the number of voices or the bytes of audio. Credentials come
//! from the environment or `~/.vigrise/credentials`.

use std::sync::Arc;

use vigrise::auth::Credentials;
use vigrise::speech::{SpeechClient, SpeechError, SynthesizeOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut args = std::env::args().skip(1);
  let endpoint = args.next().ok_or("usage: speech <gateway url> [voice]")?;
  let voice = args.next();
  let speech = SpeechClient::new(Arc::new(Credentials::resolve()?)).with_endpoint(endpoint);
  let outcome = match &voice {
    None => speech.list_voices().await.map(|voices| voices.len()),
    Some(voice) => {
      let options = SynthesizeOptions {
        format: Some(vigrise::speech::AudioFormat::Wav),
        ..Default::default()
      };
      speech
        .synthesize("Xin chào", voice, &options)
        .await
        .map(|speech| speech.audio.len())
    }
  };
  match outcome {
    Ok(size) => println!("200 {size}"),
    Err(SpeechError::Service { status, code, .. }) => println!("{status} {code}"),
    Err(other) => return Err(other.into()),
  }
  Ok(())
}
