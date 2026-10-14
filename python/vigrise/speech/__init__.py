"""Client for the Speech API and verification of its webhooks."""

from ._errors import (
    SpeechError,
    SpeechServiceError,
    SpeechSigningError,
    SpeechTransportError,
    SpeechWaitTimeoutError,
)
from ._models import (
    AudioFormat,
    AudioInfo,
    AudioUrl,
    Consent,
    ConsentStatement,
    Generation,
    GenerationSource,
    GenerationStatus,
    Job,
    JobInput,
    JobScript,
    JobStatus,
    JobText,
    Model,
    Page,
    Progress,
    Relationship,
    Speech,
    Turn,
    Voice,
    VoiceKind,
    VoiceReference,
    WebhookEvent,
    WebhookSecret,
)
from .client import DEFAULT_ENDPOINT, AsyncSpeechClient, AsyncSpeechStream, SpeechClient, SpeechStream
from .webhook import WebhookVerificationError, verify_webhook, verify_webhook_signature

__all__ = [name for name in dir() if name[:1].isupper()]
