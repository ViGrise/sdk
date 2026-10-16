# Contributing

## Checks

Run the checks of the language you changed. CI runs all of them
(`.github/workflows/ci.yml`).

```bash
# Rust (rust/vigrise, rust/vigrise-sso, rust/vigrise-cli)
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test

# TypeScript (typescript, an npm workspaces root with sdk and sso)
npm ci && npm run format:check && npm run build && npm run typecheck && npm test && npm run check:package

# Python (python)
pip install -e ".[test]" && ruff format --check . && python -m pytest -q
```

The VGR4 vectors in `test-vectors/vgr4` are generated from
`test-vectors/inputs` by `python3 test-vectors/generate.py`; CI fails when the
committed files differ from a fresh generation. Every language passes every
vector.

## Source layout

Each language keeps core signing in `auth`, code the services share in `common`,
and one directory per service.

```
rust/vigrise/src/            typescript/sdk/src/          python/vigrise/
├── auth/                    ├── auth/                    ├── auth/
├── common/                  ├── common/                  ├── _common/
└── services/                └── services/                ├── speech/
    └── <service>/               └── <service>/           └── <service>/
```

- `auth`: VGR4 signing, credentials, STS role sessions and, in Rust, the
  verifier and metering.
- `common`: the endpoint resolver, signed HTTP sending, the error envelope,
  multipart bodies and pagination. A service uses these and never repeats them.
- Inside a service: `client` (the operations), `dto` (request, option and answer
  types; one file per resource once the service grows), `enums`, `error`, and
  named domain submodules such as `speech/webhook` or `accounts/jwks`.
- Public paths do not follow the directories: Rust re-exports
  `services::<service>` as `vigrise::<service>`, and TypeScript maps
  `@vigrise/sdk/<service>` in `sdk/package.json` `exports`. Python's directory is the
  import path.

## Adding a service

A service named `<service>` (lower case, a catalog namespace) needs, in every
language, the same operations under the names of each language's convention.

- [ ] **Module in every language**, laid out as above: `services/<service>` in
      Rust behind the Cargo feature `<service>` and re-exported from `lib.rs`,
      `sdk/src/services/<service>` in `typescript` with the subpath `./<service>` in
      `sdk/package.json` `exports` (ESM and CommonJS builds, and `check:package`),
      and `vigrise/<service>` in `python` with a `<service>` extra and sync and
      async clients.
- [ ] **Endpoint resolver.** Create the client with
      `resolve_endpoint("<service>", endpoint)` (`resolveEndpoint` in
      TypeScript). The default host `https://<service>-api.vigrise.com` and the
      variables `VIGRISE_ENDPOINT_URL_<SERVICE>` and `VIGRISE_ENDPOINT_URL`
      follow without further code. Add a precedence test per language.
- [ ] **Signing.** Sign every call for the service name `<service>`; clients
      invalidate a credentials provider when the service answers 401 or 403.
- [ ] **Errors.** Surface the service's `{"error": {"code", "message"}}`
      envelope with its status and extra fields, and keep transport and signing
      failures apart.
- [ ] **Tests first.** Each language tests method, path, headers and body of
      every operation against a local fake server (Rust `axum`, TypeScript
      `node:http`, Python `httpx.MockTransport`; Python runs sync and async).
- [ ] **Shared vectors.** Behaviour that must match across languages (for
      example webhook verification) gets a vector file under `test-vectors/`
      that every language reads.
- [ ] **Docs.** `docs/<service>/README.md` with an example in every language; a
      row in the service table of `README.md`; an example under each
      language's `examples/`.
- [ ] **CI.** Clippy and tests cover the new Cargo feature; the TypeScript and
      Python jobs need no change unless the service adds a dependency.

## Style

- Two-space indentation in every language, enforced in CI: `rust/rustfmt.toml` (`cargo fmt`), Prettier in `typescript` (`npm run format`) and Ruff in `python` (`ruff format .`, line length 100). `.editorconfig` configures editors.
- Rust: edition 2024.
- Comments state what the code does and why, in the present tense.
- Write "ViGrise" in prose.

## Releasing

Every package shares one version. To release `X.Y.Z`:

1. Set `X.Y.Z` in `rust/Cargo.toml` (`[workspace.package]`), in the `vigrise`
   dependency of `rust/vigrise-cli/Cargo.toml`, in `typescript/sdk/package.json`, in `typescript/sso/package.json`
   (also its `@vigrise/sdk` dependency, `^X.Y.Z`), in the `vigrise` dependency of
   `rust/vigrise-sso/Cargo.toml` and in `python/pyproject.toml`.
2. Commit on `main` and push a `vX.Y.Z` tag.

`.github/workflows/release.yml` runs CI, checks that the tagged commit is on
`main` and that every package carries the tag's version, then publishes to
crates.io, npm and PyPI through trusted publishing once a reviewer approves the
`release` environment. npm receives staged packages: a maintainer makes them
public with 2FA through Approve on npmjs.com or `npm stage approve`. A GitHub
release with the registry links and the generated changelog follows. A registry that already holds the version is skipped, so
a failed release is retried by re-running the workflow.

Only repository admins create, move or delete `v*` tags (the "Release tags"
ruleset), and the `release` environment accepts only `v*` tags. Trusted
publishing is configured once per registry for the repository `ViGrise/sdk`,
the workflow `release.yml` and the environment `release`: on crates.io for
`vigrise`, `vigrise-sso` and `vigrise-cli`, on npm for `@vigrise/sdk` and
`@vigrise/sso`, and on PyPI for
`vigrise`.
