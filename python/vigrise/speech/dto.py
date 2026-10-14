from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional, Union

from ..auth.sts import parse_timestamp
from .enums import GenerationSource, GenerationStatus, JobStatus, Relationship, VoiceKind


def _time(value: Optional[str]) -> Optional[datetime]:
    return parse_timestamp(value) if value else None


@dataclass(frozen=True)
class Speech:
    audio: bytes
    # Content type of the audio, such as `audio/wav`.
    content_type: str
    generation_id: str
    # Samples per second, when the service reports it.
    sample_rate: Optional[int] = None


@dataclass(frozen=True)
class Voice:
    # Name of a built-in voice, or id of a cloned one.
    id: str
    kind: VoiceKind
    name: Optional[str] = None
    duration_ms: Optional[int] = None
    created_by: Optional[str] = None
    created_at: Optional[datetime] = None
    consent_id: Optional[str] = None

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Voice":
        return cls(
            id=data["id"],
            kind=VoiceKind(data["type"]),
            name=data.get("name"),
            duration_ms=data.get("duration_ms"),
            created_by=data.get("created_by"),
            created_at=_time(data.get("created_at")),
            consent_id=data.get("consent_id"),
        )


@dataclass(frozen=True)
class ConsentStatement:
    # Version to send back as `statement_version`.
    version: int
    statement: str


@dataclass(frozen=True)
class Consent:
    """The speaker's consent to clone a voice."""

    statement_version: int
    speaker_name: str
    relationship: Relationship
    agreed: bool

    def to_dict(self) -> Dict[str, Any]:
        return {
            "statement_version": self.statement_version,
            "speaker_name": self.speaker_name,
            "relationship": str(self.relationship),
            "agreed": self.agreed,
        }


@dataclass(frozen=True)
class VoiceReference:
    """A recording of the voice to clone."""

    bytes: bytes
    content_type: str
    file_name: str


@dataclass(frozen=True)
class AudioInfo:
    available: bool
    bytes: int
    expires_at: Optional[datetime] = None


@dataclass(frozen=True)
class Generation:
    id: str
    source: GenerationSource
    job_id: Optional[str]
    voice: str
    model: str
    format: str
    text: str
    characters: int
    # Characters metered for the generation.
    characters_used: int
    duration_ms: int
    status: GenerationStatus
    audio: AudioInfo
    created_by: str
    created_at: datetime

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Generation":
        audio = data["audio"]
        return cls(
            id=data["id"],
            source=GenerationSource(data["source"]),
            job_id=data.get("job_id"),
            voice=data["voice"],
            model=data["model"],
            format=data["format"],
            text=data["text"],
            characters=data["characters"],
            characters_used=data["characters_used"],
            duration_ms=data["duration_ms"],
            status=GenerationStatus(data["status"]),
            audio=AudioInfo(audio["available"], audio["bytes"], _time(audio.get("expires_at"))),
            created_by=data["created_by"],
            created_at=_time(data["created_at"]),
        )


@dataclass(frozen=True)
class AudioUrl:
    url: str
    expires_at: datetime


@dataclass(frozen=True)
class Turn:
    voice: str
    text: str


@dataclass(frozen=True)
class JobText:
    """One text in one voice."""

    text: str
    voice: str


@dataclass(frozen=True)
class JobScript:
    """Turns spoken in order, with `gap_ms` of silence between them (the service default when omitted)."""

    turns: List[Turn]
    gap_ms: Optional[int] = None


JobInput = Union[JobText, JobScript]


@dataclass(frozen=True)
class Progress:
    pieces_done: int
    pieces_total: int
    characters_done: int
    characters: int


@dataclass(frozen=True)
class Job:
    id: str
    status: JobStatus
    progress: Progress
    # Characters metered for the job so far.
    characters_used: int
    format: str
    webhook_url: Optional[str]
    error: Optional[str]
    generation_id: Optional[str]
    created_by: str
    created_at: datetime
    finished_at: Optional[datetime]

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Job":
        progress = data["progress"]
        return cls(
            id=data["id"],
            status=JobStatus(data["status"]),
            progress=Progress(progress["pieces_done"], progress["pieces_total"], progress["characters_done"], progress["characters"]),
            characters_used=data["characters_used"],
            format=data["format"],
            webhook_url=data.get("webhook_url"),
            error=data.get("error"),
            generation_id=data.get("generation_id"),
            created_by=data["created_by"],
            created_at=_time(data["created_at"]),
            finished_at=_time(data.get("finished_at")),
        )


@dataclass(frozen=True)
class Model:
    """A speech model, as the engine describes it."""

    id: str
    extra: Dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class WebhookSecret:
    secret: str = field(repr=False)
    created_at: datetime


@dataclass(frozen=True)
class WebhookEvent:
    # Id of the delivery, from the `webhook-id` header.
    id: str
    # `job.succeeded`, `job.failed` or `job.cancelled`.
    type: str
    timestamp: datetime
    # The job as it was at the event.
    data: Job
