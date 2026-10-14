//! The additional checksums of S3: CRC32, CRC32C, CRC64NVME, SHA-1 and
//! SHA-256, computed while a body streams. Values travel base64-encoded.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crc_fast::CrcAlgorithm;
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// An additional checksum algorithm of S3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChecksumAlgorithm {
  /// CRC-32.
  Crc32,
  /// CRC-32C (Castagnoli).
  Crc32c,
  /// CRC-64/NVME.
  Crc64Nvme,
  /// SHA-1.
  Sha1,
  /// SHA-256.
  Sha256,
}

impl ChecksumAlgorithm {
  /// Every supported algorithm.
  pub const ALL: [Self; 5] = [
    Self::Crc32,
    Self::Crc32c,
    Self::Crc64Nvme,
    Self::Sha1,
    Self::Sha256,
  ];

  /// The name S3 uses in `x-amz-checksum-algorithm`: `CRC32`, `CRC64NVME`, ...
  pub fn name(self) -> &'static str {
    match self {
      Self::Crc32 => "CRC32",
      Self::Crc32c => "CRC32C",
      Self::Crc64Nvme => "CRC64NVME",
      Self::Sha1 => "SHA1",
      Self::Sha256 => "SHA256",
    }
  }

  /// Finds an algorithm by its name, such as `CRC32C`, ignoring ASCII case.
  pub fn from_name(name: &str) -> Option<Self> {
    Self::ALL
      .into_iter()
      .find(|algorithm| algorithm.name().eq_ignore_ascii_case(name))
  }

  /// The request or response header carrying the value, for example
  /// `x-amz-checksum-crc32c`.
  pub fn header_name(self) -> &'static str {
    match self {
      Self::Crc32 => "x-amz-checksum-crc32",
      Self::Crc32c => "x-amz-checksum-crc32c",
      Self::Crc64Nvme => "x-amz-checksum-crc64nvme",
      Self::Sha1 => "x-amz-checksum-sha1",
      Self::Sha256 => "x-amz-checksum-sha256",
    }
  }

  /// Finds an algorithm by its header name, ignoring ASCII case.
  pub fn from_header_name(name: &str) -> Option<Self> {
    Self::ALL
      .into_iter()
      .find(|algorithm| algorithm.header_name().eq_ignore_ascii_case(name))
  }

  /// Length of the binary value.
  pub fn digest_len(self) -> usize {
    match self {
      Self::Crc32 | Self::Crc32c => 4,
      Self::Crc64Nvme => 8,
      Self::Sha1 => 20,
      Self::Sha256 => 32,
    }
  }

  /// CRC algorithms combine from part checksums; hash algorithms do not.
  pub fn is_crc(self) -> bool {
    matches!(self, Self::Crc32 | Self::Crc32c | Self::Crc64Nvme)
  }

  fn crc(self) -> Option<CrcAlgorithm> {
    match self {
      Self::Crc32 => Some(CrcAlgorithm::Crc32IsoHdlc),
      Self::Crc32c => Some(CrcAlgorithm::Crc32Iscsi),
      Self::Crc64Nvme => Some(CrcAlgorithm::Crc64Nvme),
      Self::Sha1 | Self::Sha256 => None,
    }
  }

  /// True when `value` is the base64 encoding of a digest of this algorithm.
  pub fn accepts_value(self, value: &str) -> bool {
    STANDARD
      .decode(value)
      .is_ok_and(|bytes| bytes.len() == self.digest_len())
  }
}

#[derive(Clone)]
enum State {
  Crc(crc_fast::Digest, ChecksumAlgorithm),
  Sha1(Sha1),
  Sha256(Sha256),
}

/// A checksum computed over the data passed to `update`.
#[derive(Clone)]
pub struct Checksum {
  state: State,
}

impl Checksum {
  /// Starts an empty checksum of `algorithm`.
  pub fn new(algorithm: ChecksumAlgorithm) -> Self {
    let state = match algorithm.crc() {
      Some(crc) => State::Crc(crc_fast::Digest::new(crc), algorithm),
      None if algorithm == ChecksumAlgorithm::Sha1 => State::Sha1(Sha1::new()),
      None => State::Sha256(Sha256::new()),
    };
    Self { state }
  }

  /// Feeds `data` into the checksum.
  pub fn update(&mut self, data: &[u8]) {
    match &mut self.state {
      State::Crc(digest, _) => digest.update(data),
      State::Sha1(hasher) => hasher.update(data),
      State::Sha256(hasher) => hasher.update(data),
    }
  }

  /// The binary value: big-endian for CRCs.
  pub fn finalize(self) -> Vec<u8> {
    match self.state {
      State::Crc(digest, algorithm) => {
        let value = digest.finalize();
        value.to_be_bytes()[8 - algorithm.digest_len()..].to_vec()
      }
      State::Sha1(hasher) => hasher.finalize().to_vec(),
      State::Sha256(hasher) => hasher.finalize().to_vec(),
    }
  }

  /// The base64-encoded value.
  pub fn finalize_base64(self) -> String {
    STANDARD.encode(self.finalize())
  }
}

/// The checksum of `data` in one call.
pub fn checksum_of(algorithm: ChecksumAlgorithm, data: &[u8]) -> Vec<u8> {
  let mut checksum = Checksum::new(algorithm);
  checksum.update(data);
  checksum.finalize()
}

/// The checksum of `first` followed by `second`, from the two checksums and
/// the length of `second`. `None` for hash algorithms or malformed values.
pub fn combine_crc(
  algorithm: ChecksumAlgorithm,
  first: &[u8],
  second: &[u8],
  second_len: u64,
) -> Option<Vec<u8>> {
  let crc = algorithm.crc()?;
  let width = algorithm.digest_len();
  let to_u64 = |bytes: &[u8]| {
    (bytes.len() == width).then(|| bytes.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b)))
  };
  let combined = crc_fast::checksum_combine(crc, to_u64(first)?, to_u64(second)?, second_len);
  Some(combined.to_be_bytes()[8 - width..].to_vec())
}

/// Base64-encodes a binary checksum value.
pub fn encode_base64(bytes: &[u8]) -> String {
  STANDARD.encode(bytes)
}

/// Decodes a base64 checksum value; `None` when `value` is not valid base64.
pub fn decode_base64(value: &str) -> Option<Vec<u8>> {
  STANDARD.decode(value).ok()
}

/// The composite checksum of a multipart upload: the checksum of the
/// concatenated binary part checksums, suffixed with the part count.
pub fn composite(algorithm: ChecksumAlgorithm, part_checksums: &[Vec<u8>]) -> String {
  let mut checksum = Checksum::new(algorithm);
  for part in part_checksums {
    checksum.update(part);
  }
  format!("{}-{}", checksum.finalize_base64(), part_checksums.len())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn check_string_matches_the_published_check_values() {
    let expected = [
      (ChecksumAlgorithm::Crc32, "y/Q5Jg=="),
      (ChecksumAlgorithm::Crc32c, "4waSgw=="),
      (ChecksumAlgorithm::Crc64Nvme, "rosUhgp5mIg="),
    ];
    for (algorithm, value) in expected {
      let mut checksum = Checksum::new(algorithm);
      checksum.update(b"12345");
      checksum.update(b"6789");
      assert_eq!(checksum.finalize_base64(), value, "{}", algorithm.name());
    }
  }

  #[test]
  fn names_and_headers_round_trip() {
    for algorithm in ChecksumAlgorithm::ALL {
      assert_eq!(
        ChecksumAlgorithm::from_name(algorithm.name()),
        Some(algorithm)
      );
      assert_eq!(
        ChecksumAlgorithm::from_header_name(algorithm.header_name()),
        Some(algorithm)
      );
    }
    assert_eq!(
      ChecksumAlgorithm::from_name("crc32c"),
      Some(ChecksumAlgorithm::Crc32c)
    );
    assert_eq!(ChecksumAlgorithm::from_name("MD5"), None);
  }

  #[test]
  fn combined_crcs_equal_the_crc_of_the_whole() {
    for algorithm in [
      ChecksumAlgorithm::Crc32,
      ChecksumAlgorithm::Crc32c,
      ChecksumAlgorithm::Crc64Nvme,
    ] {
      let first = checksum_of(algorithm, b"hello ");
      let second = checksum_of(algorithm, b"multipart world");
      assert_eq!(
        combine_crc(algorithm, &first, &second, 15).unwrap(),
        checksum_of(algorithm, b"hello multipart world"),
        "{}",
        algorithm.name()
      );
    }
    assert!(combine_crc(ChecksumAlgorithm::Sha256, &[0; 32], &[0; 32], 1).is_none());
    assert!(combine_crc(ChecksumAlgorithm::Crc32, &[0; 3], &[0; 4], 1).is_none());
  }

  #[test]
  fn accepts_value_checks_length_and_encoding() {
    assert!(ChecksumAlgorithm::Crc32.accepts_value("y/Q5Jg=="));
    assert!(!ChecksumAlgorithm::Crc32c.accepts_value("rosUhgp5mIg="));
    assert!(!ChecksumAlgorithm::Crc32.accepts_value("not base64!"));
  }
}
