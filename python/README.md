# vigrise

The ViGrise SDK for Python: request signing, credentials and clients for
ViGrise services, each service in its own subpackage. Requires Python 3.9; the core has no dependencies.

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

Each service has a guide with examples in
[`docs/`](https://github.com/ViGrise/sdk/tree/main/docs), and runnable examples live in
[`python/examples/`](https://github.com/ViGrise/sdk/tree/main/python/examples).

## Development

```bash
pip install -e ".[test]" && python -m pytest -q
```

## License

Apache-2.0
