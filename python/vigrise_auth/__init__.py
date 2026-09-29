"""VGR4 request signing for calling ViGrise services."""

from .credentials import Credentials, CredentialsProvider, resolve_credentials
from .signer import UNSIGNED_PAYLOAD, sign_request
from .sts import RoleSession, StsError, assume_role

__all__ = [
    "UNSIGNED_PAYLOAD",
    "Credentials",
    "CredentialsProvider",
    "RoleSession",
    "StsError",
    "assume_role",
    "resolve_credentials",
    "sign_request",
]
