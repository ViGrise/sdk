import base64
import inspect
import json
import time

import pytest

from vigrise.accounts import BackchannelLogout, InvalidTokenError, Jwks

from .signer import TestSigner

ISS = "https://accounts.vigrise.com"
EVENT = "http://schemas.openid.net/event/backchannel-logout"
SIGNER = TestSigner()
COUNTER = iter(range(10**6))


def claims():
  now = int(time.time())
  return {
    "iss": ISS,
    "sub": "user-1",
    "aud": ["vigrise-drive"],
    "iat": now,
    "exp": now + 300,
    "jti": f"jti-{next(COUNTER)}",
    "sid": "sid-1",
    "events": {EVENT: {}},
  }


def verifier():
  return BackchannelLogout(ISS, "vigrise-drive", Jwks.from_set(SIGNER.jwk_set))


@pytest.fixture(params=["verify", "averify"])
def verify(request):
  async def run(instance, token):
    result = getattr(instance, request.param)(token)
    return await result if inspect.isawaitable(result) else result

  return run


async def check(verify, mutate=lambda c: None):
  value = claims()
  mutate(value)
  return await verify(verifier(), SIGNER.sign(value))


async def test_a_valid_token_yields_sid_sub_and_jti(verify):
  result = await check(verify)
  assert (result.outcome, result.sid, result.sub) == ("fresh", "sid-1", "user-1")
  assert result.jti.startswith("jti-")


async def test_wrong_issuer_audience_event_nonce_or_subjects_are_rejected(verify):
  for mutate in (
    lambda c: c.update(iss="https://evil.example"),
    lambda c: c.update(aud=["vigrise-wallet"]),
    lambda c: c.update(events={}),
    lambda c: c.update(nonce="n"),
    lambda c: (c.pop("sid"), c.pop("sub")),
    lambda c: c.pop("jti"),
  ):
    with pytest.raises(InvalidTokenError):
      await check(verify, mutate)


async def test_the_time_window_is_enforced(verify):
  now = int(time.time())
  for mutate in (
    lambda c: c.update(exp=now - 600),
    lambda c: c.update(iat=now - 1200),
    lambda c: c.update(iat=now + 1200),
  ):
    with pytest.raises(InvalidTokenError):
      await check(verify, mutate)


async def test_tampered_foreign_and_unsigned_tokens_are_rejected(verify):
  header, _, signature = SIGNER.sign(claims()).split(".")
  other = SIGNER.sign({"iss": ISS}).split(".")[1]
  with pytest.raises(InvalidTokenError):
    await verify(verifier(), f"{header}.{other}.{signature}")
  with pytest.raises(InvalidTokenError):
    await verify(verifier(), TestSigner(SIGNER.kid).sign(claims()))
  with pytest.raises(InvalidTokenError):
    await verify(verifier(), "not-a-jwt")
  unsigned = (
    ".".join(
      base64.urlsafe_b64encode(json.dumps(part).encode()).decode().rstrip("=")
      for part in ({"alg": "none"}, claims())
    )
    + "."
  )
  with pytest.raises(InvalidTokenError):
    await verify(verifier(), unsigned)


async def test_a_repeated_jti_is_replayed(verify):
  instance = verifier()
  token = SIGNER.sign(claims())
  assert (await verify(instance, token)).outcome == "fresh"
  second = await verify(instance, token)
  assert (second.outcome, second.sid) == ("replayed", "sid-1")


async def test_events_that_are_not_an_object_are_invalid(verify):
  for events in ("logout", ["x"], 7):
    with pytest.raises(InvalidTokenError):
      await check(verify, lambda c, events=events: c.update(events=events))
