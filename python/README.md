# vigrise

The ViGrise SDK for Python 3.9+: VGR4 request signing, ViGrise Accounts,
ViGrise Storage and the Speech API. The core has no dependencies.

```sh
pip install "vigrise[speech,storage]"
```

| Extra | Pulls | For |
| --- | --- | --- |
| `requests` | `requests` | `vigrise.auth.requests_auth.ViGriseAuth` |
| `httpx` | `httpx` | `vigrise.auth.httpx_auth.ViGriseHttpxAuth` |
| `speech`, `storage` | `httpx` | `vigrise.speech`, `vigrise.storage` |
| `accounts` | `httpx`, `PyJWT[crypto]` | `vigrise.accounts` |
| `all` | every extra | |

Each client has a sync class and an async class (`SpeechClient` and
`AsyncSpeechClient`, and so on) with the same operations; they differ only in
how they send.

## Speech

```python
from vigrise.speech import JobText, SpeechClient, verify_webhook

with SpeechClient() as speech:  # endpoint, credentials optional
    spoken = speech.synthesize("Xin chào", "haley-f1", format="mp3")
    job = speech.create_job(JobText("A long text", "haley-f1"), webhook_url=webhook_url)
    done = speech.wait_for_job(job.id, timeout=60)
    audio = speech.download_generation_audio(done.generation_id)

# In the webhook handler, with the raw request body:
event = verify_webhook(secret, request.headers, raw_body)
```

`synthesize_stream` returns an iterator of chunks (an async iterator from the
async client). Errors extend `SpeechError`: `SpeechServiceError` (`status`,
`code`, `resource_id`, `details`), `SpeechTransportError`, `SpeechSigningError`,
`SpeechWaitTimeoutError` (`last_job`). Nothing is retried; pass
`idempotency_key` to repeat a call safely. Examples: `examples/speech.py`,
`examples/speech_tour.py`.

## Storage

```python
from vigrise.storage import StorageClient

with StorageClient() as storage:
    storage.create_bucket("photos")
    storage.put_object("photos", "2026/cat.jpg", data, content_type="image/jpeg")
    storage.upload_stream("photos", "big.bin", open("big.bin", "rb"))
```

Streamed uploads (`put_object_stream`, `upload_stream`) sign `UNSIGNED-PAYLOAD`;
the async client reads async iterators. Failures are `StorageError`.

## Accounts

```python
from vigrise.accounts import BackchannelLogout, Jwks, SsoIntrospector

introspector = SsoIntrospector(internal_url, client_id, client_secret)
answer = introspector.introspect_active(cookie_value)  # identity, session, accounts

logout = BackchannelLogout(issuer, client_id, Jwks.remote(jwks_url))
result = logout.verify(logout_token)  # result.outcome is "fresh" or "replayed"; averify() for async
```

`AsyncSsoIntrospector`, `OAuth2Client` / `AsyncOAuth2Client`, `Pkce`,
`AuthorizationRequest` and `IdTokenVerifier` cover the rest.

## Signing

```python
import requests
from vigrise.auth.requests_auth import ViGriseAuth
requests.get("https://speech.vigrise.com/v1/voices", auth=ViGriseAuth("speech"))
```

## Development

```bash
pip install -e ".[test]" && python -m pytest -q
```

## License

Apache-2.0
