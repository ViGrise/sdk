# vigrise

The ViGrise SDK for Python: VGR4 request signing, ViGrise Accounts, ViGrise
Storage and the Speech API. Requires Python 3.9; the core has no dependencies.

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
`AsyncSpeechClient`, and so on) with the same operations.

## Guides

[Auth](../docs/auth/README.md), [Speech](../docs/speech/README.md),
[Storage](../docs/storage/README.md) and [Accounts](../docs/accounts/README.md).
Examples: `examples/speech.py`, `examples/speech_tour.py`.

## Development

```bash
pip install -e ".[test]" && python -m pytest -q
```

## License

Apache-2.0
