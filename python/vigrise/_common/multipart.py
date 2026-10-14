import hashlib
from typing import List, Optional, Tuple

# name, body, file name, content type
Part = Tuple[str, bytes, Optional[str], Optional[str]]


def encode_multipart(parts: List[Part]) -> Tuple[str, bytes]:
  """The `Content-Type` header value and the `multipart/form-data` body. The boundary is derived from the parts, so it cannot occur inside them."""
  for _, _, _, content_type in parts:
    if content_type is not None and ("\r" in content_type or "\n" in content_type):
      raise ValueError("a content type cannot contain line breaks")
  salt = 0
  while True:
    digest = hashlib.sha256(str(salt).encode())
    for _, body, _, _ in parts:
      digest.update(body)
    boundary = "vigrise-" + digest.hexdigest()[:32]
    if not any(boundary.encode() in body for _, body, _, _ in parts):
      break
    salt += 1
  out = bytearray()
  for name, body, file_name, content_type in parts:
    out += f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"'.encode()
    if file_name is not None:
      out += f'; filename="{file_name.replace(chr(34), "%22").replace(chr(13), "%0D").replace(chr(10), "%0A")}"\r\nContent-Type: {content_type}'.encode()
    out += b"\r\n\r\n" + body + b"\r\n"
  out += f"--{boundary}--\r\n".encode()
  return f"multipart/form-data; boundary={boundary}", bytes(out)
