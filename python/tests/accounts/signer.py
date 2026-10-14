import json
import random

import jwt
from cryptography.hazmat.primitives.asymmetric import rsa
from jwt.algorithms import RSAAlgorithm


class TestSigner:
  """An RSA key that signs tokens the way ViGrise Accounts does."""

  __test__ = False

  def __init__(self, kid=None):
    self.kid = kid or "kid-%08x" % random.getrandbits(32)
    self._private = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    jwk = json.loads(RSAAlgorithm.to_jwk(self._private.public_key()))
    jwk.update({"kid": self.kid, "alg": "RS256", "use": "sig"})
    self.jwk = jwk

  @property
  def jwk_set(self):
    return {"keys": [self.jwk]}

  def sign(self, claims, headers=None):
    return jwt.encode(
      claims, self._private, algorithm="RS256", headers={"kid": self.kid, **(headers or {})}
    )
