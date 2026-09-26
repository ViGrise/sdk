"""VGR4 request signing for calling ViGrise services."""

from .credentials import Credentials, CredentialsProvider, resolve_credentials
from .signer import sign_request
from .sts import RoleSession, StsError, assume_role

__all__ = [
    "Credentials",
    "CredentialsProvider",
    "RoleSession",
    "StsError",
    "assume_role",
    "resolve_credentials",
    "sign_request",
]
