#![cfg(feature = "sigv4")]

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::{Body, Frame};
use http_body_util::BodyExt;
use vigrise::auth::canonical::hex_sha256;
use vigrise::auth::checksum::{checksum_of, encode_base64, ChecksumAlgorithm};
use vigrise::auth::chunked::{
  AwsChunkedBody, ChunkSigning, ChunkedError, ChunkedMode, ChunkedOptions, Trailers,
};
use vigrise::auth::sigv4::{chunk_signature, derive_signing_key, trailer_signature, Scope};

/// Hands out its frames one by one, so chunk boundaries fall anywhere.
struct Frames(VecDeque<Bytes>);

impl Body for Frames {
  type Data = Bytes;
  type Error = std::convert::Infallible;

  fn poll_frame(
    mut self: Pin<&mut Self>,
    _cx: &mut Context<'_>,
  ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
    Poll::Ready(self.0.pop_front().map(|bytes| Ok(Frame::data(bytes))))
  }
}

fn frames(wire: &[u8], size: usize) -> Frames {
  Frames(wire.chunks(size).map(Bytes::copy_from_slice).collect())
}

const TIMESTAMP: &str = "20130524T000000Z";
const SEED: &str = "4f232c4386841ef735655705268965c44a0e4690baa4adea153f7db9fa80a0a9";

fn signing() -> ChunkSigning {
  ChunkSigning {
    signing_key: derive_signing_key(
      "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
      "20130524",
      "us-east-1",
      "s3",
    ),
    timestamp: TIMESTAMP.into(),
    scope: Scope::new("20130524", "us-east-1", "s3"),
    seed_signature: SEED.into(),
  }
}

/// Encodes `chunks` in the signed aws-chunked format, followed by the final
/// empty chunk and, when given, a signed trailer.
fn signed_wire(chunks: &[&[u8]], trailer: Option<&str>) -> Vec<u8> {
  let signing = signing();
  let mut previous = signing.seed_signature.clone();
  let mut wire = Vec::new();
  for data in chunks.iter().copied().chain([&b""[..]]) {
    previous = chunk_signature(
      &signing.signing_key,
      TIMESTAMP,
      &signing.scope,
      &previous,
      &hex_sha256(data),
    );
    wire.extend(format!("{:x};chunk-signature={previous}\r\n", data.len()).into_bytes());
    wire.extend(data);
    wire.extend(b"\r\n");
  }
  if let Some(line) = trailer {
    let signature = trailer_signature(
      &signing.signing_key,
      TIMESTAMP,
      &signing.scope,
      &previous,
      format!("{line}\n").as_bytes(),
    );
    // The final chunk's terminating CRLF is the blank line only without a trailer.
    wire.truncate(wire.len() - 2);
    wire.extend(format!("{line}\r\nx-amz-trailer-signature:{signature}\r\n\r\n").into_bytes());
  }
  wire
}

fn unsigned_wire(chunks: &[&[u8]], trailer: &str) -> Vec<u8> {
  let mut wire = Vec::new();
  for data in chunks {
    wire.extend(format!("{:x}\r\n", data.len()).into_bytes());
    wire.extend(*data);
    wire.extend(b"\r\n");
  }
  wire.extend(format!("0\r\n{trailer}\r\n").into_bytes());
  wire
}

fn options(mode: ChunkedMode, decoded_length: Option<u64>) -> ChunkedOptions {
  ChunkedOptions {
    mode,
    decoded_length,
    trailer_checksum: None,
  }
}

async fn decode(
  wire: &[u8],
  frame_size: usize,
  options: ChunkedOptions,
) -> (Result<Vec<u8>, ChunkedError>, Trailers) {
  let (body, trailers) = AwsChunkedBody::new(frames(wire, frame_size), options);
  let result = body
    .collect()
    .await
    .map(|collected| collected.to_bytes().to_vec());
  (result, trailers)
}

fn crc32c_trailer(data: &[u8]) -> String {
  format!(
    "x-amz-checksum-crc32c:{}\r\n",
    encode_base64(&checksum_of(ChecksumAlgorithm::Crc32c, data))
  )
}

#[tokio::test]
async fn signed_chunks_decode_at_any_read_boundary() {
  let (a, b) = (vec![b'a'; 65536], vec![b'a'; 1024]);
  let wire = signed_wire(&[&a, &b], None);
  for frame_size in [1, 7, 100, 65536, wire.len()] {
    let (decoded, _) = decode(
      &wire,
      frame_size,
      options(ChunkedMode::Signed(signing()), Some(66560)),
    )
    .await;
    assert_eq!(decoded.unwrap().len(), 66560, "frame size {frame_size}");
  }
}

#[tokio::test]
async fn a_tampered_chunk_fails_its_signature() {
  let mut wire = signed_wire(&[b"hello world"], None);
  let at = wire.windows(5).position(|w| w == b"hello").unwrap();
  wire[at] = b'H';
  let (decoded, _) = decode(&wire, 16, options(ChunkedMode::Signed(signing()), None)).await;
  assert_eq!(decoded.unwrap_err(), ChunkedError::SignatureMismatch);
}

#[tokio::test]
async fn a_chunk_signed_in_the_wrong_order_fails() {
  let wire = signed_wire(&[b"one", b"two"], None);
  let text = String::from_utf8(wire).unwrap();
  let mut lines: Vec<&str> = text.split("\r\n").collect();
  lines.swap(0, 2);
  lines.swap(1, 3);
  let swapped = lines.join("\r\n").into_bytes();
  let (decoded, _) = decode(&swapped, 64, options(ChunkedMode::Signed(signing()), None)).await;
  assert_eq!(decoded.unwrap_err(), ChunkedError::SignatureMismatch);
}

#[tokio::test]
async fn a_missing_chunk_signature_is_malformed() {
  let (decoded, _) = decode(
    b"5\r\nhello\r\n0\r\n\r\n",
    64,
    options(ChunkedMode::Signed(signing()), None),
  )
  .await;
  assert!(matches!(decoded.unwrap_err(), ChunkedError::Malformed(_)));
}

#[tokio::test]
async fn a_body_cut_before_the_final_chunk_is_truncated() {
  let wire = signed_wire(&[b"hello world"], None);
  let cut = &wire[..wire.len() - 90];
  let (decoded, _) = decode(cut, 32, options(ChunkedMode::Signed(signing()), None)).await;
  assert_eq!(decoded.unwrap_err(), ChunkedError::Truncated);
}

#[tokio::test]
async fn a_short_body_differs_from_the_declared_decoded_length() {
  let wire = signed_wire(&[b"hello"], None);
  let (decoded, _) = decode(&wire, 64, options(ChunkedMode::Signed(signing()), Some(6))).await;
  assert_eq!(decoded.unwrap_err(), ChunkedError::Truncated);
}

#[tokio::test]
async fn a_signed_trailer_is_verified_and_exposed() {
  let data = b"payload with a trailer";
  let trailer = crc32c_trailer(data);
  let wire = signed_wire(&[data], Some(trailer.trim_end()));
  let (decoded, trailers) = decode(
    &wire,
    5,
    options(
      ChunkedMode::SignedTrailer(signing()),
      Some(data.len() as u64),
    ),
  )
  .await;
  assert_eq!(decoded.unwrap(), data);
  assert_eq!(
    trailers.get("X-Amz-Checksum-CRC32C").as_deref(),
    Some(trailer.trim_end().split_once(':').unwrap().1)
  );
}

#[tokio::test]
async fn a_forged_trailer_fails_its_signature() {
  let data = b"payload with a trailer";
  let wire = signed_wire(&[data], Some(crc32c_trailer(data).trim_end()));
  let forged = String::from_utf8(wire)
    .unwrap()
    .replace(
      crc32c_trailer(data).trim_end(),
      crc32c_trailer(b"other").trim_end(),
    )
    .into_bytes();
  let (decoded, _) = decode(
    &forged,
    64,
    options(ChunkedMode::SignedTrailer(signing()), None),
  )
  .await;
  assert_eq!(decoded.unwrap_err(), ChunkedError::SignatureMismatch);
}

#[tokio::test]
async fn a_signed_trailer_body_without_the_trailer_signature_is_malformed() {
  let data = b"x";
  let mut wire = signed_wire(&[data], None);
  wire.truncate(wire.len() - 2);
  wire.extend(format!("{}\r\n\r\n", crc32c_trailer(data).trim_end()).into_bytes());
  let (decoded, _) = decode(
    &wire,
    64,
    options(ChunkedMode::SignedTrailer(signing()), None),
  )
  .await;
  assert!(matches!(decoded.unwrap_err(), ChunkedError::Malformed(_)));
}

#[tokio::test]
async fn unsigned_chunks_stream_through_with_the_trailer_checked() {
  let data = b"streamed without chunk signatures";
  let wire = unsigned_wire(&[&data[..10], &data[10..]], &crc32c_trailer(data));
  let mut opts = options(ChunkedMode::UnsignedTrailer, Some(data.len() as u64));
  opts.trailer_checksum = Some(ChecksumAlgorithm::Crc32c);
  for frame_size in [1, 3, wire.len()] {
    let (decoded, trailers) = decode(&wire, frame_size, opts.clone()).await;
    assert_eq!(decoded.unwrap(), data, "frame size {frame_size}");
    assert!(trailers.get("x-amz-checksum-crc32c").is_some());
  }
}

#[tokio::test]
async fn a_wrong_trailer_checksum_fails_the_body() {
  let data = b"streamed without chunk signatures";
  let wire = unsigned_wire(&[data], &crc32c_trailer(b"something else"));
  let mut opts = options(ChunkedMode::UnsignedTrailer, None);
  opts.trailer_checksum = Some(ChecksumAlgorithm::Crc32c);
  let (decoded, _) = decode(&wire, 64, opts).await;
  assert!(matches!(
    decoded.unwrap_err(),
    ChunkedError::ChecksumMismatch { .. }
  ));
}

#[tokio::test]
async fn a_missing_trailer_checksum_fails_when_one_is_expected() {
  let wire = unsigned_wire(&[b"data"], "");
  let mut opts = options(ChunkedMode::UnsignedTrailer, None);
  opts.trailer_checksum = Some(ChecksumAlgorithm::Crc32c);
  let (decoded, _) = decode(&wire, 64, opts).await;
  assert!(matches!(decoded.unwrap_err(), ChunkedError::Malformed(_)));
}

#[tokio::test]
async fn a_bad_chunk_size_line_is_malformed() {
  let (decoded, _) = decode(
    b"zz\r\nabc\r\n0\r\n\r\n",
    64,
    options(ChunkedMode::UnsignedTrailer, None),
  )
  .await;
  assert!(matches!(decoded.unwrap_err(), ChunkedError::Malformed(_)));
}

#[tokio::test]
async fn a_chunk_not_followed_by_crlf_is_malformed() {
  let (decoded, _) = decode(
    b"3\r\nabcXX0\r\n\r\n",
    64,
    options(ChunkedMode::UnsignedTrailer, None),
  )
  .await;
  assert!(matches!(decoded.unwrap_err(), ChunkedError::Malformed(_)));
}

#[tokio::test]
async fn data_after_the_final_chunk_is_refused() {
  let mut wire = unsigned_wire(&[b"data"], "");
  wire.extend(b"trailing bytes");
  for frame_size in [1, wire.len()] {
    let (decoded, _) = decode(
      &wire,
      frame_size,
      options(ChunkedMode::UnsignedTrailer, None),
    )
    .await;
    assert!(
      matches!(decoded.unwrap_err(), ChunkedError::Malformed(_)),
      "frame size {frame_size}"
    );
  }
  let mut signed = signed_wire(&[b"data"], None);
  signed.extend(b"x");
  let (decoded, _) = decode(&signed, 7, options(ChunkedMode::Signed(signing()), None)).await;
  assert!(matches!(decoded.unwrap_err(), ChunkedError::Malformed(_)));
}

#[tokio::test]
async fn an_endless_trailer_is_refused() {
  let mut wire = b"0\r\n".to_vec();
  for i in 0..2000 {
    wire.extend(format!("x-filler-{i}:value\r\n").into_bytes());
  }
  wire.extend(b"\r\n");
  let (decoded, _) = decode(&wire, 64, options(ChunkedMode::UnsignedTrailer, None)).await;
  assert!(matches!(decoded.unwrap_err(), ChunkedError::Malformed(_)));
}

#[tokio::test]
async fn chunk_sizes_are_plain_hex_digits() {
  for size in ["+4", "-4", "0x4", " 4"] {
    let wire = format!("{size}\r\ndata\r\n0\r\n\r\n").into_bytes();
    let (decoded, _) = decode(&wire, 64, options(ChunkedMode::UnsignedTrailer, None)).await;
    assert!(
      matches!(decoded.unwrap_err(), ChunkedError::Malformed(_)),
      "accepted {size:?}"
    );
  }
}
