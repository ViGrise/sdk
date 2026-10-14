"""Client for the Speech API and verification of its webhooks."""

from .errors import (
  SpeechError,
  SpeechServiceError,
  SpeechSigningError,
  SpeechTransportError,
  SpeechWaitTimeoutError,
)
from .._common.pagination import Page
from .dto import (
  AudioInfo,
  AudioUrl,
  Consent,
  ConsentStatement,
  Generation,
  Job,
  JobInput,
  JobScript,
  JobText,
  Model,
  Progress,
  Speech,
  Turn,
  Voice,
  VoiceReference,
  WebhookEvent,
  WebhookSecret,
)
from .enums import (
  AudioFormat,
  GenerationSource,
  GenerationStatus,
  JobStatus,
  Relationship,
  VoiceKind,
)
from .webhook import WebhookVerificationError, verify_webhook, verify_webhook_signature

_CLIENT_NAMES = ("AsyncSpeechClient", "AsyncSpeechStream", "SpeechClient", "SpeechStream")

__all__ = [
  "AsyncSpeechClient",
  "AsyncSpeechStream",
  "AudioFormat",
  "AudioInfo",
  "AudioUrl",
  "Consent",
  "ConsentStatement",
  "Generation",
  "GenerationSource",
  "GenerationStatus",
  "Job",
  "JobInput",
  "JobScript",
  "JobStatus",
  "JobText",
  "Model",
  "Page",
  "Progress",
  "Relationship",
  "Speech",
  "SpeechClient",
  "SpeechError",
  "SpeechServiceError",
  "SpeechSigningError",
  "SpeechStream",
  "SpeechTransportError",
  "SpeechWaitTimeoutError",
  "Turn",
  "Voice",
  "VoiceKind",
  "VoiceReference",
  "WebhookEvent",
  "WebhookSecret",
  "WebhookVerificationError",
  "verify_webhook",
  "verify_webhook_signature",
]


def __getattr__(name):
  # The clients import httpx; webhook verification works without it.
  if name in _CLIENT_NAMES:
    from . import client

    return getattr(client, name)
  raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
