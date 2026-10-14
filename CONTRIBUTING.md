# Contributing

## Checks

Run the checks of the language you changed. CI runs all of them
(`.github/workflows/ci.yml`).

```bash
# Rust (rust/vigrise, rust/vigrise-cli)
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test

# TypeScript (typescript)
npm ci && npm run typecheck && npm test && npm run build && npm run check:package

# Python (python)
pip install -e ".[test]" && python -m pytest -q
```

The VGR4 vectors in `test-vectors/vgr4` are generated from
`test-vectors/inputs` by `python3 test-vectors/generate.py`; CI fails when the
committed files differ from a fresh generation. Every language passes every
vector.

## Adding a service

A service named `<service>` (lower case, a catalog namespace) needs, in all
three languages, the same operations under the names of each language's
convention.

- [ ] **Module in three languages.** `vigrise::<service>` behind the Cargo
      feature `<service>` (directory module: client, types, errors, options),
      `src/<service>` in `typescript` with the subpath `./<service>` in
      `package.json` `exports` (ESM and CommonJS builds, and `check:package`),
      and `vigrise.<service>` in `python` with a `<service>` extra and sync and
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
      that all three languages read.
- [ ] **Docs.** `docs/<service>/README.md` with an example in Rust, TypeScript
      and Python; a row in the service table of `README.md`; a short section in
      each language README; an example under each language's `examples/`.
- [ ] **CI.** Clippy and tests cover the new Cargo feature; the TypeScript and
      Python jobs need no change unless the service adds a dependency.

## Style

- Rust: edition 2024, `rustfmt` with two-space indentation (`.rustfmt.toml`).
- Comments state what the code does and why, in the present tense.
- Write "ViGrise" in prose.
