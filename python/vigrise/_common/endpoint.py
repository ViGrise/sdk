"""Where a ViGrise service lives: the explicit endpoint, then `VIGRISE_ENDPOINT_URL_<SERVICE>`, then
`VIGRISE_ENDPOINT_URL`, then `https://<service>-api.vigrise.com`."""

import os
from typing import Mapping, Optional

ASSUME_ROLE_PATH = "/v1/assume-role"


def env_key(service: str) -> str:
  """The environment variable of one service, such as `VIGRISE_ENDPOINT_URL_VOICE_STUDIO` for `voice-studio`."""
  return "VIGRISE_ENDPOINT_URL_" + service.upper().replace("-", "_")


def resolve_endpoint(
  service: str, explicit: Optional[str] = None, env: Optional[Mapping[str, str]] = None
) -> str:
  """The endpoint of `service`. Empty values count as unset and a trailing slash is dropped."""
  env = os.environ if env is None else env
  endpoint = (
    explicit
    or env.get(env_key(service))
    or env.get("VIGRISE_ENDPOINT_URL")
    or f"https://{service}-api.vigrise.com"
  )
  return endpoint.rstrip("/")


def sts_url(explicit: Optional[str] = None, env: Optional[Mapping[str, str]] = None) -> str:
  """The URL of the role session call. The STS endpoint is accepted with or without the `/v1/assume-role` path."""
  endpoint = resolve_endpoint("sts", explicit, env)
  return endpoint if endpoint.endswith(ASSUME_ROLE_PATH) else endpoint + ASSUME_ROLE_PATH


def sts_base_url(explicit: Optional[str] = None, env: Optional[Mapping[str, str]] = None) -> str:
  """The base URL of the STS endpoint, without the `/v1/assume-role` path."""
  endpoint = resolve_endpoint("sts", explicit, env)
  return endpoint[: -len(ASSUME_ROLE_PATH)] if endpoint.endswith(ASSUME_ROLE_PATH) else endpoint
