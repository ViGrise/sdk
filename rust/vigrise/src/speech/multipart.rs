use bytes::{BufMut, Bytes, BytesMut};
use sha2::{Digest, Sha256};

/// A `multipart/form-data` body.
pub(crate) struct Multipart {
  parts: Vec<Part>,
}

struct Part {
  name: &'static str,
  file: Option<(String, String)>,
  body: Bytes,
}

impl Multipart {
  pub(crate) fn new() -> Self {
    Self { parts: Vec::new() }
  }

  pub(crate) fn text(mut self, name: &'static str, value: impl Into<Bytes>) -> Self {
    self.parts.push(Part {
      name,
      file: None,
      body: value.into(),
    });
    self
  }

  pub(crate) fn file(
    mut self,
    name: &'static str,
    file_name: &str,
    content_type: &str,
    body: Bytes,
  ) -> Self {
    self.parts.push(Part {
      name,
      file: Some((file_name.to_string(), content_type.to_string())),
      body,
    });
    self
  }

  /// The `Content-Type` header value and the encoded body. The boundary is
  /// derived from the parts, so it cannot occur inside them.
  pub(crate) fn encode(&self) -> (String, Bytes) {
    let mut salt = 0u32;
    let boundary = loop {
      let mut digest = Sha256::new();
      digest.update(salt.to_be_bytes());
      for part in &self.parts {
        digest.update(&part.body);
      }
      let candidate = format!("vigrise-{}", hex::encode(&digest.finalize()[..16]));
      if !self
        .parts
        .iter()
        .any(|part| contains(&part.body, candidate.as_bytes()))
      {
        break candidate;
      }
      salt += 1;
    };
    let mut out = BytesMut::new();
    for part in &self.parts {
      out.put_slice(format!("--{boundary}\r\n").as_bytes());
      match &part.file {
        Some((file_name, content_type)) => out.put_slice(
          format!(
            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {content_type}\r\n\r\n",
            part.name,
            file_name.replace('"', "%22").replace('\r', "%0D").replace('\n', "%0A")
          )
          .as_bytes(),
        ),
        None => out.put_slice(
          format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", part.name).as_bytes(),
        ),
      }
      out.put_slice(&part.body);
      out.put_slice(b"\r\n");
    }
    out.put_slice(format!("--{boundary}--\r\n").as_bytes());
    (
      format!("multipart/form-data; boundary={boundary}"),
      out.freeze(),
    )
  }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
  haystack
    .windows(needle.len())
    .any(|window| window == needle)
}
