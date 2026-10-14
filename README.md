# ViGrise SDK

Libraries and a command line for calling ViGrise services. Every language
signs requests the same way (VGR4) and offers the same clients and operations
for each ViGrise service.

## Layout

| Path | Contents |
| --- | --- |
| `rust/vigrise` | The `vigrise` crate |
| `rust/vigrise-cli` | The `vigrise` command line: `login`, `logout`, `whoami` |
| `typescript` | The `@vigrise/sdk` package |
| `python` | The `vigrise` package |
| `test-vectors` | Reference implementation and the VGR4 and webhook vectors every language passes |
| `docs` | One guide per service |

## Languages

| Language | Install | Requires | Guide |
| --- | --- | --- | --- |
| Rust | `cargo add vigrise --features speech,storage` | Rust 1.85, edition 2024 | [rust/vigrise](rust/vigrise/README.md) |
| TypeScript | `npm install @vigrise/sdk` | Node 24 | [typescript](typescript/README.md) |
| Python | `pip install "vigrise[speech,storage]"` | Python 3.9 | [python](python/README.md) |
| Command line | `cargo install vigrise-cli` | | [rust/vigrise-cli](rust/vigrise-cli/README.md) |

## Services

| Service | What it does | Guide |
| --- | --- | --- |
| Auth | VGR4 signing, credentials, role sessions, sign-in from the command line | [docs/auth](docs/auth/README.md) |
| Speech | Text to speech, voice cloning, jobs and webhooks | [docs/speech](docs/speech/README.md) |
| Storage | Buckets, objects, multipart uploads, presigned URLs | [docs/storage](docs/storage/README.md) |
| Accounts | SSO introspection, back-channel logout, OAuth2 and OIDC | [docs/accounts](docs/accounts/README.md) |

## Credentials

Clients read credentials from `VIGRISE_ACCESS_KEY_ID` and
`VIGRISE_SECRET_ACCESS_KEY` (with `VIGRISE_SESSION_TOKEN` for a role session),
then from profile `VIGRISE_PROFILE` (default `default`) of
`~/.vigrise/credentials`. `vigrise login` writes a role session to a profile.
Details: [docs/auth](docs/auth/README.md#credentials).

## Endpoints

Each service lives on `https://<service>-api.vigrise.com`. A client resolves its
endpoint from its `endpoint` argument, then `VIGRISE_ENDPOINT_URL_<SERVICE>`
(for example `VIGRISE_ENDPOINT_URL_SPEECH`), then `VIGRISE_ENDPOINT_URL` for
every service, then that default.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0
