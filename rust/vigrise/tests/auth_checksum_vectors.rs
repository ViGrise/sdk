#![cfg(feature = "sigv4")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use vigrise::auth::checksum::{
  checksum_of, combine_crc, composite, encode_base64, ChecksumAlgorithm,
};

#[derive(Deserialize)]
struct Vectors {
  checksums: Vec<Known>,
  composite: Vec<Composite>,
  full_object: Vec<Composite>,
}

#[derive(Deserialize)]
struct Known {
  input: String,
  length: usize,
  data_utf8: Option<String>,
  repeat: Option<Repeat>,
  values: BTreeMap<String, String>,
}

#[derive(Deserialize, Clone)]
struct Repeat {
  byte: String,
  length: usize,
}

#[derive(Deserialize)]
struct Composite {
  algorithm: String,
  parts: Vec<Repeat>,
  value: String,
}

fn vectors() -> Vectors {
  let path =
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-vectors/checksum/vectors.json");
  serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn part_data(part: &Repeat) -> Vec<u8> {
  part.byte.as_bytes().repeat(part.length)
}

#[test]
fn known_values_match() {
  for known in vectors().checksums {
    let data = match (&known.data_utf8, &known.repeat) {
      (Some(text), _) => text.clone().into_bytes(),
      (None, Some(repeat)) => part_data(repeat),
      _ => panic!("{}: no data", known.input),
    };
    assert_eq!(data.len(), known.length);
    for (name, value) in &known.values {
      let algorithm = ChecksumAlgorithm::from_name(name).unwrap();
      assert_eq!(
        &encode_base64(&checksum_of(algorithm, &data)),
        value,
        "{} {name}",
        known.input
      );
    }
  }
}

#[test]
fn composite_checksums_match() {
  for case in vectors().composite {
    let algorithm = ChecksumAlgorithm::from_name(&case.algorithm).unwrap();
    let part_checksums: Vec<Vec<u8>> = case
      .parts
      .iter()
      .map(|part| checksum_of(algorithm, &part_data(part)))
      .collect();
    assert_eq!(
      composite(algorithm, &part_checksums),
      case.value,
      "{}",
      case.algorithm
    );
  }
}

#[test]
fn full_object_checksums_combine_from_part_checksums() {
  for case in vectors().full_object {
    let algorithm = ChecksumAlgorithm::from_name(&case.algorithm).unwrap();
    let mut parts = case.parts.iter();
    let first = parts.next().unwrap();
    let mut combined = checksum_of(algorithm, &part_data(first));
    for part in parts {
      combined = combine_crc(
        algorithm,
        &combined,
        &checksum_of(algorithm, &part_data(part)),
        part.length as u64,
      )
      .unwrap();
    }
    assert_eq!(encode_base64(&combined), case.value, "{}", case.algorithm);
  }
}
