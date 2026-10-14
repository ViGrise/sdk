import asyncio
import time
from typing import AsyncIterator, Iterator, List, Optional

import httpx

from .._common import http as _http
from .._common.senders import AsyncSender, Failures, SyncSender
from ..auth.credentials import CredentialsSource
from .._common.endpoint import resolve_endpoint
from . import operations as ops
from .errors import (
  SpeechSigningError,
  SpeechTransportError,
  SpeechWaitTimeoutError,
  error_from_response,
)
from .._common.pagination import Page
from .dto import (
  AudioUrl,
  Consent,
  ConsentStatement,
  Generation,
  Job,
  JobInput,
  Model,
  Speech,
  Voice,
  VoiceReference,
  WebhookSecret,
)
from .enums import (
  AudioFormat,
  GenerationSource,
  JobStatus,
)


class SpeechStream:
  """Audio sent while it is produced, with the metadata of the answer. Iterate it for the chunks and close it when done."""

  def __init__(self, response: httpx.Response):
    self._response = response
    self.content_type, self.generation_id, self.sample_rate = ops.audio_metadata(response)

  def __iter__(self) -> Iterator[bytes]:
    try:
      yield from self._response.iter_bytes()
    except httpx.HTTPError as error:
      raise SpeechTransportError(str(error)) from error

  def close(self) -> None:
    self._response.close()

  def __enter__(self) -> "SpeechStream":
    return self

  def __exit__(self, *exc_info) -> None:
    self.close()


class AsyncSpeechStream:
  """Audio sent while it is produced, with the metadata of the answer. Iterate it for the chunks and close it when done."""

  def __init__(self, response: httpx.Response):
    self._response = response
    self.content_type, self.generation_id, self.sample_rate = ops.audio_metadata(response)

  async def __aiter__(self) -> AsyncIterator[bytes]:
    try:
      async for chunk in self._response.aiter_bytes():
        yield chunk
    except httpx.HTTPError as error:
      raise SpeechTransportError(str(error)) from error

  async def aclose(self) -> None:
    await self._response.aclose()

  async def __aenter__(self) -> "AsyncSpeechStream":
    return self

  async def __aexit__(self, *exc_info) -> None:
    await self.aclose()


_FAILURES = Failures(SpeechSigningError, SpeechTransportError, error_from_response)


class SpeechClient:
  """A client for the Speech API. Every call is VGR4-signed for the `speech` service."""

  def __init__(
    self,
    endpoint: Optional[str] = None,
    credentials: Optional[CredentialsSource] = None,
    *,
    timeout: Optional[float] = 60.0,
    transport: Optional[httpx.BaseTransport] = None,
  ):
    self._sender = SyncSender(
      "speech", resolve_endpoint("speech", endpoint), credentials, timeout, _FAILURES, transport
    )

  def close(self) -> None:
    self._sender.close()

  def __enter__(self) -> "SpeechClient":
    return self

  def __exit__(self, *exc_info) -> None:
    self.close()

  def _send(self, request: _http.Request, stream: bool = False) -> httpx.Response:
    return self._sender.send(request, stream)

  def synthesize(
    self,
    text: str,
    voice: str,
    *,
    model: Optional[str] = None,
    format: Optional[AudioFormat] = None,
    store_audio: Optional[bool] = None,
    idempotency_key: Optional[str] = None,
  ) -> Speech:
    """Synthesizes `text` in `voice` and returns the whole audio."""
    return ops.parse_speech(
      self._send(
        ops.synthesize_request(text, voice, False, model, format, store_audio, idempotency_key)
      )
    )

  def synthesize_stream(
    self,
    text: str,
    voice: str,
    *,
    model: Optional[str] = None,
    format: Optional[AudioFormat] = None,
    store_audio: Optional[bool] = None,
    idempotency_key: Optional[str] = None,
  ) -> SpeechStream:
    """Synthesizes `text` in `voice` and returns the audio as it is produced."""
    return SpeechStream(
      self._send(
        ops.synthesize_request(text, voice, True, model, format, store_audio, idempotency_key),
        stream=True,
      )
    )

  def list_voices(self) -> List[Voice]:
    """The built-in voices of the project and its cloned voices."""
    return ops.parse_voices(self._send(ops.list_voices_request()))

  def get_voice(self, voice_id: str) -> Voice:
    return ops.parse_voice(self._send(ops.get_voice_request(voice_id)))

  def delete_voice(self, voice_id: str) -> None:
    self._send(ops.delete_voice_request(voice_id))

  def clone_voice(
    self,
    name: str,
    reference: VoiceReference,
    consent: Consent,
    *,
    idempotency_key: Optional[str] = None,
  ) -> Voice:
    """Clones a voice from a recording of its speaker, who has given `consent`."""
    return ops.parse_voice(
      self._send(ops.clone_voice_request(name, reference, consent, idempotency_key))
    )

  def consent_statement(self) -> ConsentStatement:
    """The statement a speaker agrees to before a voice is cloned."""
    return ops.parse_consent_statement(self._send(ops.consent_statement_request()))

  def list_generations(
    self,
    *,
    cursor: Optional[str] = None,
    limit: Optional[int] = None,
    source: Optional[GenerationSource] = None,
    voice: Optional[str] = None,
  ) -> Page[Generation]:
    return ops.parse_generation_page(
      self._send(ops.list_generations_request(cursor, limit, source, voice))
    )

  def get_generation(self, generation_id: str) -> Generation:
    return ops.parse_generation(self._send(ops.get_generation_request(generation_id)))

  def delete_generation(self, generation_id: str, *, all: bool = False) -> None:
    """Deletes a generation; with `all`, also the other generations of its job."""
    self._send(ops.delete_generation_request(generation_id, all))

  def generation_audio_url(self, generation_id: str, *, download: bool = False) -> AudioUrl:
    """A temporary URL to download the audio of a generation from; with `download`, browsers save it as a file."""
    return ops.parse_audio_url(
      self._send(ops.generation_audio_url_request(generation_id, download))
    )

  def download_generation_audio(self, generation_id: str) -> bytes:
    """Downloads the audio of a generation."""
    location = self.generation_audio_url(generation_id)
    try:
      response = self._sender.http.get(location.url, timeout=self._sender.timeout)
    except httpx.HTTPError as error:
      raise SpeechTransportError(str(error)) from error
    return ops.check(response).content

  def create_job(
    self,
    input: JobInput,
    *,
    model: Optional[str] = None,
    format: Optional[AudioFormat] = None,
    webhook_url: Optional[str] = None,
    store_audio: Optional[bool] = None,
    idempotency_key: Optional[str] = None,
  ) -> Job:
    """Starts a synthesis that runs in the background."""
    return ops.parse_job(
      self._send(
        ops.create_job_request(input, model, format, webhook_url, store_audio, idempotency_key)
      )
    )

  def get_job(self, job_id: str) -> Job:
    return ops.parse_job(self._send(ops.get_job_request(job_id)))

  def list_jobs(
    self,
    *,
    cursor: Optional[str] = None,
    limit: Optional[int] = None,
    status: Optional[JobStatus] = None,
  ) -> Page[Job]:
    return ops.parse_job_page(self._send(ops.list_jobs_request(cursor, limit, status)))

  def cancel_job(self, job_id: str) -> Job:
    """Cancels a job that has not finished."""
    return ops.parse_job(self._send(ops.cancel_job_request(job_id)))

  def wait_for_job(
    self, job_id: str, *, poll_interval: float = 2.0, timeout: Optional[float] = None
  ) -> Job:
    """Polls a job until it succeeded, failed or was cancelled. On a timeout the error carries the last job."""
    started = time.monotonic()
    while True:
      job = self.get_job(job_id)
      if job.status.is_finished:
        return job
      if timeout is not None and time.monotonic() - started + poll_interval > timeout:
        raise SpeechWaitTimeoutError(job)
      time.sleep(poll_interval)

  def list_models(self) -> List[Model]:
    """The speech models of the engine."""
    return ops.parse_models(self._send(ops.list_models_request()))

  def rotate_webhook_secret(self) -> WebhookSecret:
    """Creates the webhook secret of the project, or replaces the current one."""
    return ops.parse_webhook_secret(self._send(ops.rotate_webhook_secret_request()))


class AsyncSpeechClient:
  """The async counterpart of `SpeechClient`, with the same operations."""

  def __init__(
    self,
    endpoint: Optional[str] = None,
    credentials: Optional[CredentialsSource] = None,
    *,
    timeout: Optional[float] = 60.0,
    transport: Optional[httpx.AsyncBaseTransport] = None,
  ):
    self._sender = AsyncSender(
      "speech", resolve_endpoint("speech", endpoint), credentials, timeout, _FAILURES, transport
    )

  async def aclose(self) -> None:
    await self._sender.aclose()

  async def __aenter__(self) -> "AsyncSpeechClient":
    return self

  async def __aexit__(self, *exc_info) -> None:
    await self.aclose()

  async def _send(self, request: _http.Request, stream: bool = False) -> httpx.Response:
    return await self._sender.send(request, stream)

  async def synthesize(
    self,
    text: str,
    voice: str,
    *,
    model: Optional[str] = None,
    format: Optional[AudioFormat] = None,
    store_audio: Optional[bool] = None,
    idempotency_key: Optional[str] = None,
  ) -> Speech:
    """Synthesizes `text` in `voice` and returns the whole audio."""
    return ops.parse_speech(
      await self._send(
        ops.synthesize_request(text, voice, False, model, format, store_audio, idempotency_key)
      )
    )

  async def synthesize_stream(
    self,
    text: str,
    voice: str,
    *,
    model: Optional[str] = None,
    format: Optional[AudioFormat] = None,
    store_audio: Optional[bool] = None,
    idempotency_key: Optional[str] = None,
  ) -> AsyncSpeechStream:
    """Synthesizes `text` in `voice` and returns the audio as it is produced."""
    return AsyncSpeechStream(
      await self._send(
        ops.synthesize_request(text, voice, True, model, format, store_audio, idempotency_key),
        stream=True,
      )
    )

  async def list_voices(self) -> List[Voice]:
    """The built-in voices of the project and its cloned voices."""
    return ops.parse_voices(await self._send(ops.list_voices_request()))

  async def get_voice(self, voice_id: str) -> Voice:
    return ops.parse_voice(await self._send(ops.get_voice_request(voice_id)))

  async def delete_voice(self, voice_id: str) -> None:
    await self._send(ops.delete_voice_request(voice_id))

  async def clone_voice(
    self,
    name: str,
    reference: VoiceReference,
    consent: Consent,
    *,
    idempotency_key: Optional[str] = None,
  ) -> Voice:
    """Clones a voice from a recording of its speaker, who has given `consent`."""
    return ops.parse_voice(
      await self._send(ops.clone_voice_request(name, reference, consent, idempotency_key))
    )

  async def consent_statement(self) -> ConsentStatement:
    """The statement a speaker agrees to before a voice is cloned."""
    return ops.parse_consent_statement(await self._send(ops.consent_statement_request()))

  async def list_generations(
    self,
    *,
    cursor: Optional[str] = None,
    limit: Optional[int] = None,
    source: Optional[GenerationSource] = None,
    voice: Optional[str] = None,
  ) -> Page[Generation]:
    return ops.parse_generation_page(
      await self._send(ops.list_generations_request(cursor, limit, source, voice))
    )

  async def get_generation(self, generation_id: str) -> Generation:
    return ops.parse_generation(await self._send(ops.get_generation_request(generation_id)))

  async def delete_generation(self, generation_id: str, *, all: bool = False) -> None:
    """Deletes a generation; with `all`, also the other generations of its job."""
    await self._send(ops.delete_generation_request(generation_id, all))

  async def generation_audio_url(self, generation_id: str, *, download: bool = False) -> AudioUrl:
    """A temporary URL to download the audio of a generation from; with `download`, browsers save it as a file."""
    return ops.parse_audio_url(
      await self._send(ops.generation_audio_url_request(generation_id, download))
    )

  async def download_generation_audio(self, generation_id: str) -> bytes:
    """Downloads the audio of a generation."""
    location = await self.generation_audio_url(generation_id)
    try:
      response = await self._sender.http.get(location.url, timeout=self._sender.timeout)
    except httpx.HTTPError as error:
      raise SpeechTransportError(str(error)) from error
    return ops.check(response).content

  async def create_job(
    self,
    input: JobInput,
    *,
    model: Optional[str] = None,
    format: Optional[AudioFormat] = None,
    webhook_url: Optional[str] = None,
    store_audio: Optional[bool] = None,
    idempotency_key: Optional[str] = None,
  ) -> Job:
    """Starts a synthesis that runs in the background."""
    return ops.parse_job(
      await self._send(
        ops.create_job_request(input, model, format, webhook_url, store_audio, idempotency_key)
      )
    )

  async def get_job(self, job_id: str) -> Job:
    return ops.parse_job(await self._send(ops.get_job_request(job_id)))

  async def list_jobs(
    self,
    *,
    cursor: Optional[str] = None,
    limit: Optional[int] = None,
    status: Optional[JobStatus] = None,
  ) -> Page[Job]:
    return ops.parse_job_page(await self._send(ops.list_jobs_request(cursor, limit, status)))

  async def cancel_job(self, job_id: str) -> Job:
    """Cancels a job that has not finished."""
    return ops.parse_job(await self._send(ops.cancel_job_request(job_id)))

  async def wait_for_job(
    self, job_id: str, *, poll_interval: float = 2.0, timeout: Optional[float] = None
  ) -> Job:
    """Polls a job until it succeeded, failed or was cancelled. On a timeout the error carries the last job."""
    started = time.monotonic()
    while True:
      job = await self.get_job(job_id)
      if job.status.is_finished:
        return job
      if timeout is not None and time.monotonic() - started + poll_interval > timeout:
        raise SpeechWaitTimeoutError(job)
      await asyncio.sleep(poll_interval)

  async def list_models(self) -> List[Model]:
    """The speech models of the engine."""
    return ops.parse_models(await self._send(ops.list_models_request()))

  async def rotate_webhook_secret(self) -> WebhookSecret:
    """Creates the webhook secret of the project, or replaces the current one."""
    return ops.parse_webhook_secret(await self._send(ops.rotate_webhook_secret_request()))
