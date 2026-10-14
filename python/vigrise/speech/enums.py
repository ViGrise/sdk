"""Strings the Speech API sends."""

from enum import Enum


class OpenEnum(str, Enum):
    """Strings a service sends. A value this version of the SDK does not know becomes a member of its own."""

    @classmethod
    def _missing_(cls, value):
        member = str.__new__(cls, value)
        member._name_ = str(value)
        member._value_ = value
        return member

    def __str__(self) -> str:
        return str(self.value)


class AudioFormat(OpenEnum):
    WAV = "wav"
    PCM = "pcm"
    MP3 = "mp3"
    OPUS = "opus"


class GenerationSource(OpenEnum):
    SYNC = "sync"
    STREAM = "stream"
    JOB = "job"


class GenerationStatus(OpenEnum):
    COMPLETE = "complete"
    INCOMPLETE = "incomplete"


class JobStatus(OpenEnum):
    QUEUED = "queued"
    RUNNING = "running"
    SUCCEEDED = "succeeded"
    FAILED = "failed"
    CANCELLED = "cancelled"

    @property
    def is_finished(self) -> bool:
        """Whether the job will not change any more."""
        return self.value in ("succeeded", "failed", "cancelled")


class VoiceKind(OpenEnum):
    SYSTEM = "system"
    CLONED = "cloned"


class Relationship(OpenEnum):
    """How the speaker of a reference recording relates to the person cloning the voice."""

    SELF = "self"
    AUTHORIZED = "authorized"
