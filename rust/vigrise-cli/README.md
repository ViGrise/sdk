# vigrise-cli

The `vigrise` command line: sign in with your ViGrise account and get role
session credentials that every ViGrise SDK reads.

## Installation

```sh
cargo install vigrise-cli
```

The binary is named `vigrise`.

## Example

```sh
vigrise login --role vrn:vigrise:iam:<project_id>:role/<name> --profile dev
vigrise whoami --profile dev
```

`vigrise login` signs you in at ViGrise Accounts and writes a role session to a
profile of `~/.vigrise/credentials`. It opens a browser; `--device` shows a code
to enter on another device instead. Without `--role` it lists the roles you may
assume. The sign-in is cached in `~/.vigrise/sso/<profile>.json`, so the next
`vigrise login` needs no browser until it expires.
`vigrise login --select-account` skips the cached sign-in and opens the ViGrise
account chooser in the browser, so another account can take over the profile.

| Command | Does |
| --- | --- |
| `vigrise login` | Signs in and writes a role session to a profile |
| `vigrise whoami` | Shows the role and expiry of a profile's session |
| `vigrise logout` | Removes a profile's session and cached sign-in |

A role lets you in when its trust policy names you as a project member, for
example `"Principal": {"VGR": "vrn:vigrise:iam:<project_id>:member/*"}`.

## STS endpoint

`vigrise login` asks the STS endpoint for the role session: `--sts-url`, then
`VIGRISE_ENDPOINT_URL_STS`, then `VIGRISE_ENDPOINT_URL`, then
`https://sts-api.vigrise.com`.

## Credential profiles

Sessions are written to the profile named by `--profile` or `VIGRISE_PROFILE`
(default `default`) in `VIGRISE_SHARED_CREDENTIALS_FILE` (default
`~/.vigrise/credentials`). `vigrise login` refuses a profile that holds a long-term access key; choose
another profile.

## Library

The crate also exposes the sign-in flow (`vigrise_cli::login`) and the
credentials file handling (`vigrise_cli::profile`). API reference:
<https://docs.rs/vigrise-cli>.

## License

Apache-2.0
