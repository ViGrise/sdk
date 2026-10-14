"""Where each ViGrise service lives."""

from .._common.endpoint import ASSUME_ROLE_PATH, env_key, resolve_endpoint, sts_base_url, sts_url

__all__ = ["ASSUME_ROLE_PATH", "env_key", "resolve_endpoint", "sts_base_url", "sts_url"]
