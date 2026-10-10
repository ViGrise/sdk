//! Decoding of `aws-chunked` request bodies.
//!
//! The decoder wraps the raw body and yields the decoded object bytes. A bad
//! chunk, a bad signature, a bad trailer, a checksum that differs from the
//! trailer or a body that ends early fails the stream, so a consumer that
//! reads to the end never commits such a body.

use std::error::Error;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};

use bytes::{Buf, Bytes, BytesMut};
use http_body::{Body, Frame};

use crate::canonical::hex_sha256;
use crate::checksum::{Checksum, ChecksumAlgorithm};
use crate::signing::verify_signature;
use crate::sigv4::{chunk_string_to_sign, trailer_string_to_sign, Scope};

/// Longest chunk a signed body may carry; every signed chunk is held in memory
/// until its signature is checked.
const MAX_SIGNED_CHUNK_BYTES: u64 = 16 * 1024 * 1024;
/// Longest chunk header or trailer line.
const MAX_LINE_BYTES: usize = 8 * 1024;
/// Longest trailer, all its lines together.
const MAX_TRAILER_BYTES: usize = 16 * 1024;

/// What a signed body's chunk signatures chain from.
#[derive(Clone)]
pub struct ChunkSigning {
  /// Signing key of the credential scope.
  pub signing_key: [u8; 32],
  /// Request timestamp.
  pub timestamp: String,
  /// Credential scope.
  pub scope: Scope,
  /// The signature of the request's `Authorization` header.
  pub seed_signature: String,
}

impl std::fmt::Debug for ChunkSigning {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ChunkSigning")
      .field("signing_key", &"<redacted>")
      .field("timestamp", &self.timestamp)
      .field("scope", &self.scope)
      .finish_non_exhaustive()
  }
}

/// The `x-amz-content-sha256` payload modes that use aws-chunked bodies.
#[derive(Debug, Clone)]
pub enum ChunkedMode {
  /// `STREAMING-UNSIGNED-PAYLOAD-TRAILER`
  UnsignedTrailer,
  /// `STREAMING-AWS4-HMAC-SHA256-PAYLOAD`
  Signed(ChunkSigning),
  /// `STREAMING-AWS4-HMAC-SHA256-PAYLOAD-TRAILER`
  SignedTrailer(ChunkSigning),
}

/// How to decode and verify an aws-chunked body.
#[derive(Debug, Clone)]
pub struct ChunkedOptions {
  /// The payload mode the request declared.
  pub mode: ChunkedMode,
  /// `x-amz-decoded-content-length`, the size of the object.
  pub decoded_length: Option<u64>,
  /// The checksum whose value the trailer must carry; the decoded bytes are
  /// checked against it at the end.
  pub trailer_checksum: Option<ChecksumAlgorithm>,
}

/// Why an aws-chunked body could not be decoded or verified.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChunkedError {
  /// The framing of the body is invalid.
  #[error("{0}")]
  Malformed(String),
  /// The body ended before the declared content was complete.
  #[error("The request body ended before the declared content was complete")]
  Truncated,
  /// A chunk or trailer signature does not match.
  #[error("The chunk or trailer signature does not match")]
  SignatureMismatch,
  /// The checksum in the trailer does not match the decoded bytes.
  #[error("The {algorithm} checksum sent in the trailer does not match the body")]
  ChecksumMismatch {
    /// Name of the checksum algorithm.
    algorithm: &'static str,
  },
  /// The inner body failed while being read.
  #[error("The request body could not be read: {0}")]
  Read(String),
}

/// Trailing headers of a decoded body, available once the body has ended.
#[derive(Debug, Clone, Default)]
pub struct Trailers(Arc<OnceLock<Vec<(String, String)>>>);

impl Trailers {
  /// The value of the trailing header `name`, ignoring ASCII case.
  pub fn get(&self, name: &str) -> Option<String> {
    self
      .0
      .get()?
      .iter()
      .find(|(n, _)| n.eq_ignore_ascii_case(name))
      .map(|(_, value)| value.clone())
  }
}

enum Phase {
  ChunkHeader,
  ChunkData {
    remaining: u64,
    signature: Option<String>,
  },
  ChunkEnd,
  FinalEnd,
  Trailer,
  Done,
}

enum Step {
  Data(Bytes),
  NeedMore,
  Finished,
}

/// Decodes an aws-chunked body into a stream of its content, verifying chunk
/// signatures and the trailer checksum as the bytes pass.
pub struct AwsChunkedBody<B> {
  inner: B,
  inner_ended: bool,
  failed: bool,
  buffer: BytesMut,
  phase: Phase,
  signing: Option<ChunkSigning>,
  has_trailer: bool,
  previous_signature: String,
  decoded: u64,
  decoded_length: Option<u64>,
  checksum: Option<(ChecksumAlgorithm, Checksum)>,
  trailers: Trailers,
}

impl<B> AwsChunkedBody<B> {
  /// Wraps `inner`; the returned [`Trailers`] fill once the body has ended.
  pub fn new(inner: B, options: ChunkedOptions) -> (Self, Trailers) {
    let (signing, has_trailer) = match options.mode {
      ChunkedMode::UnsignedTrailer => (None, true),
      ChunkedMode::Signed(signing) => (Some(signing), false),
      ChunkedMode::SignedTrailer(signing) => (Some(signing), true),
    };
    let trailers = Trailers::default();
    let body = Self {
      inner,
      inner_ended: false,
      failed: false,
      buffer: BytesMut::new(),
      phase: Phase::ChunkHeader,
      previous_signature: signing
        .as_ref()
        .map(|s| s.seed_signature.clone())
        .unwrap_or_default(),
      signing,
      has_trailer,
      decoded: 0,
      decoded_length: options.decoded_length,
      checksum: options
        .trailer_checksum
        .map(|algorithm| (algorithm, Checksum::new(algorithm))),
      trailers: trailers.clone(),
    };
    (body, trailers)
  }

  fn take_line(&mut self) -> Result<Option<String>, ChunkedError> {
    let Some(end) = self.buffer.windows(2).position(|pair| pair == b"\r\n") else {
      if self.buffer.len() > MAX_LINE_BYTES {
        return Err(ChunkedError::Malformed(
          "A chunk header line is too long".into(),
        ));
      }
      return Ok(None);
    };
    if end > MAX_LINE_BYTES {
      return Err(ChunkedError::Malformed(
        "A chunk header line is too long".into(),
      ));
    }
    let line = self.buffer.split_to(end);
    self.buffer.advance(2);
    String::from_utf8(line.to_vec())
      .map(Some)
      .map_err(|_| ChunkedError::Malformed("A chunk header line is not valid text".into()))
  }

  fn parse_chunk_header(&self, line: &str) -> Result<(u64, Option<String>), ChunkedError> {
    let malformed = || ChunkedError::Malformed("Malformed chunk header".into());
    let (size, extension) = match line.split_once(';') {
      Some((size, extension)) => (size, Some(extension)),
      None => (line, None),
    };
    if size.is_empty() || !size.bytes().all(|b| b.is_ascii_hexdigit()) {
      return Err(malformed());
    }
    let size = u64::from_str_radix(size, 16).map_err(|_| malformed())?;
    let signature = match (&self.signing, extension) {
      (Some(_), Some(extension)) => {
        let value = extension
          .strip_prefix("chunk-signature=")
          .ok_or_else(malformed)?;
        Some(value.to_string())
      }
      (Some(_), None) => {
        return Err(ChunkedError::Malformed(
          "A signed chunk is missing its signature".into(),
        ))
      }
      (None, _) => None,
    };
    if self.signing.is_some() && size > MAX_SIGNED_CHUNK_BYTES {
      return Err(ChunkedError::Malformed(format!(
        "A signed chunk exceeds {MAX_SIGNED_CHUNK_BYTES} bytes"
      )));
    }
    Ok((size, signature))
  }

  fn verify_chunk(&mut self, signature: &str, data: &[u8]) -> Result<(), ChunkedError> {
    let signing = self
      .signing
      .as_ref()
      .expect("signed chunks have a signing context");
    let message = chunk_string_to_sign(
      &signing.timestamp,
      &signing.scope,
      &self.previous_signature,
      &hex_sha256(data),
    );
    if !verify_signature(&signing.signing_key, &message, signature) {
      return Err(ChunkedError::SignatureMismatch);
    }
    self.previous_signature = signature.to_ascii_lowercase();
    Ok(())
  }

  fn emit(&mut self, data: Bytes) -> Result<Step, ChunkedError> {
    self.decoded += data.len() as u64;
    if let Some(limit) = self.decoded_length {
      if self.decoded > limit {
        return Err(ChunkedError::Malformed(
          "The body is longer than x-amz-decoded-content-length".into(),
        ));
      }
    }
    if let Some((_, checksum)) = &mut self.checksum {
      checksum.update(&data);
    }
    Ok(Step::Data(data))
  }

  fn step(&mut self) -> Result<Step, ChunkedError> {
    loop {
      match &mut self.phase {
        Phase::ChunkHeader => {
          let Some(line) = self.take_line()? else {
            return Ok(Step::NeedMore);
          };
          let (size, signature) = self.parse_chunk_header(&line)?;
          if size == 0 {
            if let Some(signature) = signature {
              self.verify_chunk(&signature, b"")?;
            }
            self.phase = if self.has_trailer {
              Phase::Trailer
            } else {
              Phase::FinalEnd
            };
          } else {
            self.phase = Phase::ChunkData {
              remaining: size,
              signature,
            };
          }
        }
        Phase::ChunkData {
          remaining,
          signature,
        } => {
          if let Some(signature) = signature.clone() {
            let needed = *remaining as usize;
            if self.buffer.len() < needed {
              return Ok(Step::NeedMore);
            }
            let data = self.buffer.split_to(needed).freeze();
            self.verify_chunk(&signature, &data)?;
            self.phase = Phase::ChunkEnd;
            return self.emit(data);
          }
          if self.buffer.is_empty() {
            return Ok(Step::NeedMore);
          }
          let take = (*remaining).min(self.buffer.len() as u64) as usize;
          *remaining -= take as u64;
          if *remaining == 0 {
            self.phase = Phase::ChunkEnd;
          }
          let data = self.buffer.split_to(take).freeze();
          return self.emit(data);
        }
        Phase::ChunkEnd => {
          if self.buffer.len() < 2 {
            return Ok(Step::NeedMore);
          }
          if &self.buffer[..2] != b"\r\n" {
            return Err(ChunkedError::Malformed(
              "A chunk is not followed by CRLF".into(),
            ));
          }
          self.buffer.advance(2);
          self.phase = Phase::ChunkHeader;
        }
        Phase::FinalEnd => {
          if self.buffer.len() < 2 {
            return Ok(Step::NeedMore);
          }
          if &self.buffer[..2] != b"\r\n" {
            return Err(ChunkedError::Malformed(
              "The final chunk is not followed by CRLF".into(),
            ));
          }
          self.buffer.advance(2);
          self.finish(Vec::new())?;
          return Ok(Step::Finished);
        }
        Phase::Trailer => {
          let Some(lines) = self.take_trailer_lines()? else {
            return Ok(Step::NeedMore);
          };
          let signature = if self.signing.is_some() {
            let last = lines.last().filter(|line| {
              line
                .split_once(':')
                .is_some_and(|(name, _)| name.eq_ignore_ascii_case("x-amz-trailer-signature"))
            });
            let signature = last
              .and_then(|line| line.split_once(':'))
              .map(|(_, value)| value.trim().to_string())
              .ok_or_else(|| {
                ChunkedError::Malformed("The trailer is missing x-amz-trailer-signature".into())
              })?;
            Some(signature)
          } else {
            None
          };
          let headers: Vec<(String, String)> = lines
            .iter()
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
            .filter(|(name, _)| name != "x-amz-trailer-signature")
            .collect();
          if let Some(signature) = &signature {
            let signing = self.signing.as_ref().expect("a signature implies signing");
            let text: String = headers
              .iter()
              .map(|(name, value)| format!("{name}:{value}\n"))
              .collect();
            let message = trailer_string_to_sign(
              &signing.timestamp,
              &signing.scope,
              &self.previous_signature,
              text.as_bytes(),
            );
            if !verify_signature(&signing.signing_key, &message, signature) {
              return Err(ChunkedError::SignatureMismatch);
            }
          }
          self.finish(headers)?;
          return Ok(Step::Finished);
        }
        Phase::Done => return Ok(Step::Finished),
      }
    }
  }

  /// Reads trailer lines up to the blank line; `None` while incomplete.
  fn take_trailer_lines(&mut self) -> Result<Option<Vec<String>>, ChunkedError> {
    let mut consumed = 0usize;
    let mut lines = Vec::new();
    loop {
      let rest = &self.buffer[consumed..];
      let Some(end) = rest.windows(2).position(|pair| pair == b"\r\n") else {
        if rest.len() > MAX_LINE_BYTES || consumed + rest.len() > MAX_TRAILER_BYTES {
          return Err(ChunkedError::Malformed("The trailer is too long".into()));
        }
        return Ok(None);
      };
      let line = String::from_utf8(rest[..end].to_vec())
        .map_err(|_| ChunkedError::Malformed("A trailer line is not valid text".into()))?;
      consumed += end + 2;
      if consumed > MAX_TRAILER_BYTES {
        return Err(ChunkedError::Malformed("The trailer is too long".into()));
      }
      if line.is_empty() {
        self.buffer.advance(consumed);
        return Ok(Some(lines));
      }
      lines.push(line);
    }
  }

  fn finish(&mut self, trailer: Vec<(String, String)>) -> Result<(), ChunkedError> {
    self.phase = Phase::Done;
    if let Some(declared) = self.decoded_length {
      if self.decoded < declared {
        return Err(ChunkedError::Truncated);
      }
    }
    if let Some((algorithm, checksum)) = self.checksum.take() {
      let name = algorithm.header_name();
      let sent = trailer
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, value)| value.as_str())
        .ok_or_else(|| ChunkedError::Malformed(format!("The trailer is missing {name}")))?;
      if checksum.finalize_base64() != sent {
        return Err(ChunkedError::ChecksumMismatch {
          algorithm: algorithm.name(),
        });
      }
    }
    let _ = self.trailers.0.set(trailer);
    Ok(())
  }

  /// The input ended while the decoder wanted more.
  fn end_of_input(&mut self) -> Result<Step, ChunkedError> {
    match self.phase {
      Phase::FinalEnd | Phase::Trailer if self.buffer.is_empty() && self.signing.is_none() => {
        self.finish(Vec::new())?;
        Ok(Step::Finished)
      }
      Phase::Done => Ok(Step::Finished),
      _ => Err(ChunkedError::Truncated),
    }
  }
}

fn after_the_end() -> ChunkedError {
  ChunkedError::Malformed("The body carries data after its last chunk".into())
}

impl<B> Body for AwsChunkedBody<B>
where
  B: Body<Data = Bytes> + Unpin,
  B::Error: Into<Box<dyn Error + Send + Sync>>,
{
  type Data = Bytes;
  type Error = ChunkedError;

  fn poll_frame(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
    let this = &mut *self;
    if this.failed {
      return Poll::Ready(None);
    }
    loop {
      let step = match this.step() {
        Ok(Step::NeedMore) if this.inner_ended => this.end_of_input(),
        other => other,
      };
      match step {
        Err(error) => {
          this.failed = true;
          return Poll::Ready(Some(Err(error)));
        }
        Ok(Step::Data(bytes)) => return Poll::Ready(Some(Ok(Frame::data(bytes)))),
        Ok(Step::Finished) => {
          if !this.buffer.is_empty() {
            this.failed = true;
            return Poll::Ready(Some(Err(after_the_end())));
          }
          if this.inner_ended {
            return Poll::Ready(None);
          }
          return match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
              this.inner_ended = true;
              Poll::Ready(None)
            }
            Poll::Ready(Some(Ok(frame))) if frame.data_ref().is_none_or(|data| data.is_empty()) => {
              cx.waker().wake_by_ref();
              Poll::Pending
            }
            Poll::Ready(Some(Ok(_))) => {
              this.failed = true;
              Poll::Ready(Some(Err(after_the_end())))
            }
            Poll::Ready(Some(Err(error))) => {
              this.failed = true;
              Poll::Ready(Some(Err(ChunkedError::Read(error.into().to_string()))))
            }
          };
        }
        Ok(Step::NeedMore) => {}
      }
      match Pin::new(&mut this.inner).poll_frame(cx) {
        Poll::Pending => return Poll::Pending,
        Poll::Ready(None) => this.inner_ended = true,
        Poll::Ready(Some(Err(error))) => {
          this.failed = true;
          return Poll::Ready(Some(Err(ChunkedError::Read(error.into().to_string()))));
        }
        Poll::Ready(Some(Ok(frame))) => {
          if let Ok(data) = frame.into_data() {
            this.buffer.extend_from_slice(&data);
          }
        }
      }
    }
  }

  fn is_end_stream(&self) -> bool {
    self.failed || matches!(self.phase, Phase::Done)
  }
}
