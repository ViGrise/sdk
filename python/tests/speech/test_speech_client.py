import inspect
import json
import re
from datetime import datetime, timezone

import httpx
import pytest

from vigrise.auth import Credentials
from vigrise.speech import (
    AsyncSpeechClient,
    Consent,
    GenerationSource,
    JobScript,
    JobStatus,
    JobText,
    Relationship,
    SpeechClient,
    SpeechServiceError,
    SpeechSigningError,
    SpeechTransportError,
    SpeechWaitTimeoutError,
    Turn,
    VoiceKind,
    VoiceReference,
)

CREDENTIALS = Credentials("VGAKTEST", "secret")


def job(status):
    return {
        "id": "job_1", "object": "job", "status": status,
        "progress": {"pieces_done": 1, "pieces_total": 2, "characters_done": 5, "characters": 10},
        "characters_used": 5, "format": "wav", "webhook_url": None, "error": None, "generation_id": None,
        "created_by": "me", "created_at": "2026-10-01T12:00:00Z", "finished_at": None,
    }


GENERATION = {
    "id": "gen_1", "object": "generation", "source": "sync", "job_id": None, "voice": "haley-f1", "model": "fp16",
    "format": "wav", "text": "Xin", "characters": 3, "characters_used": 3, "duration_ms": 900, "status": "complete",
    "audio": {"available": True, "bytes": 44, "expires_at": "2026-10-02T12:00:00Z"},
    "created_by": "me", "created_at": "2026-10-01T12:00:00Z",
}

VOICE = {"id": "voice_1", "object": "voice", "type": "cloned", "name": "Mine", "duration_ms": 8000,
         "created_by": "me", "created_at": "2026-10-01T12:00:00Z"}


@pytest.fixture(params=[SpeechClient, AsyncSpeechClient], ids=["sync", "async"])
def make_client(request):
    """Builds a client of the class under test around a handler; returns (client, requests seen)."""

    def build(handler):
        seen = []

        def record(http_request):
            seen.append(http_request)
            return handler(http_request)

        client = request.param(
            endpoint="https://speech.test", credentials=CREDENTIALS, transport=httpx.MockTransport(record)
        )
        return client, seen

    return build


async def call(client, name, *args, **kwargs):
    result = getattr(client, name)(*args, **kwargs)
    return await result if inspect.isawaitable(result) else result


async def collect(stream):
    chunks = []
    if hasattr(stream, "__aiter__"):
        async for chunk in stream:
            chunks.append(chunk)
    else:
        for chunk in stream:
            chunks.append(chunk)
    return b"".join(chunks)


def json_response(body, status=200):
    return httpx.Response(status, json=body)


async def test_every_call_is_signed_vgr4_for_speech(make_client):
    client, seen = make_client(lambda r: json_response({"object": "list", "data": []}))
    await call(client, "list_voices")
    headers = seen[-1].headers
    assert re.match(r"^VGR4-HMAC-SHA256 Credential=VGAKTEST/\d{8}/speech/vgr4_request, ", headers["authorization"])
    assert "x-vgr-date" in headers and "x-vgr-content-sha256" in headers


async def test_synthesize_posts_the_request_and_reads_the_audio_headers(make_client):
    client, seen = make_client(
        lambda r: httpx.Response(
            200, content=b"RIFFaudio",
            headers={"content-type": "audio/wav", "x-vgr-generation-id": "gen_9", "x-audio-sample-rate": "24000"},
        )
    )
    speech = await call(
        client, "synthesize", "Xin chào", "haley-f1",
        model="fp16", format="mp3", store_audio=False, idempotency_key="key-1",
    )
    assert speech.audio == b"RIFFaudio"
    assert (speech.content_type, speech.generation_id, speech.sample_rate) == ("audio/wav", "gen_9", 24000)
    request = seen[-1]
    assert (request.method, request.url.path) == ("POST", "/v1/audio/speech")
    assert request.headers["idempotency-key"] == "key-1"
    assert request.headers["content-type"] == "application/json"
    assert json.loads(request.content) == {
        "input": "Xin chào", "voice": "haley-f1", "model": "fp16", "response_format": "mp3", "store_audio": False,
    }


async def test_synthesize_leaves_out_what_is_not_set(make_client):
    client, seen = make_client(
        lambda r: httpx.Response(200, content=b"a", headers={"content-type": "audio/wav", "x-vgr-generation-id": "g"})
    )
    speech = await call(client, "synthesize", "hi", "haley-f1")
    assert speech.sample_rate is None
    assert json.loads(seen[-1].content) == {"input": "hi", "voice": "haley-f1"}
    assert "idempotency-key" not in seen[-1].headers


class Chunks(httpx.SyncByteStream, httpx.AsyncByteStream):
    """A body of two chunks, readable by sync and async clients."""

    def __iter__(self):
        yield b"abc"
        yield b"def"

    async def __aiter__(self):
        yield b"abc"
        yield b"def"


async def test_synthesize_stream_yields_the_chunks_and_the_metadata(make_client):
    client, seen = make_client(
        lambda r: httpx.Response(
            200, stream=Chunks(),
            headers={"content-type": "audio/pcm", "x-vgr-generation-id": "gen_s", "x-audio-sample-rate": "16000"},
        )
    )
    stream = await call(client, "synthesize_stream", "hi", "haley-f1")
    assert (stream.content_type, stream.generation_id, stream.sample_rate) == ("audio/pcm", "gen_s", 16000)
    assert await collect(stream) == b"abcdef"
    assert json.loads(seen[-1].content)["stream"] is True


async def test_voices_are_listed_read_and_deleted(make_client):
    def handler(request):
        if request.method == "DELETE":
            return httpx.Response(204)
        if request.url.path == "/v1/voices":
            return json_response({"object": "list", "data": [
                {"id": "haley-f1", "object": "voice", "type": "system"}, VOICE,
            ]})
        return json_response(VOICE)

    client, seen = make_client(handler)
    voices = await call(client, "list_voices")
    assert [voice.kind for voice in voices] == [VoiceKind.SYSTEM, VoiceKind.CLONED]
    assert (voices[1].name, voices[1].duration_ms) == ("Mine", 8000)
    assert (await call(client, "get_voice", "voice_1")).duration_ms == 8000
    await call(client, "delete_voice", "voice_1")
    assert (seen[-1].method, seen[-1].url.path) == ("DELETE", "/v1/voices/voice_1")


async def test_clone_voice_sends_name_reference_and_consent_as_multipart(make_client):
    client, seen = make_client(lambda r: json_response(VOICE, 201))
    voice = await call(
        client, "clone_voice", "Mine",
        VoiceReference(b"RIFF\x00\x01", "audio/wav", "me.wav"),
        Consent(statement_version=1, speaker_name="An", relationship=Relationship.SELF, agreed=True),
        idempotency_key="clone-1",
    )
    assert voice.id == "voice_1"
    request = seen[-1]
    assert (request.method, request.url.path) == ("POST", "/v1/voices")
    assert request.headers["idempotency-key"] == "clone-1"
    boundary = re.match(r"^multipart/form-data; boundary=(.+)$", request.headers["content-type"]).group(1)
    body = request.content.decode("latin-1")
    assert body.startswith(f"--{boundary}\r\n") and body.endswith(f"--{boundary}--\r\n")
    assert 'Content-Disposition: form-data; name="name"\r\n\r\nMine\r\n' in body
    assert (
        'Content-Disposition: form-data; name="reference"; filename="me.wav"\r\nContent-Type: audio/wav\r\n\r\nRIFF\x00\x01\r\n'
        in body
    )
    consent = re.search(r'name="consent"\r\n\r\n(.*?)\r\n--', body, re.S).group(1)
    assert json.loads(consent) == {"statement_version": 1, "speaker_name": "An", "relationship": "self", "agreed": True}


async def test_the_consent_statement_is_read(make_client):
    client, seen = make_client(lambda r: json_response({"version": 3, "statement": "I agree"}))
    statement = await call(client, "consent_statement")
    assert (statement.version, statement.statement) == (3, "I agree")
    assert seen[-1].url.path == "/v1/consent-statement"


async def test_generations_are_listed_read_and_deleted(make_client):
    def handler(request):
        if request.method == "DELETE":
            return httpx.Response(204)
        if request.url.path == "/v1/generations":
            return json_response({"object": "list", "data": [GENERATION], "next_cursor": "c2"})
        return json_response(GENERATION)

    client, seen = make_client(handler)
    page = await call(client, "list_generations", cursor="c1", limit=5, source=GenerationSource.JOB, voice="haley f1")
    assert page.next_cursor == "c2"
    assert (page.data[0].source, page.data[0].audio.bytes) == (GenerationSource.SYNC, 44)
    assert page.data[0].created_at == datetime(2026, 10, 1, 12, tzinfo=timezone.utc)
    assert seen[-1].url.query == b"cursor=c1&limit=5&source=job&voice=haley%20f1"
    assert (await call(client, "get_generation", "gen_1")).id == "gen_1"
    await call(client, "delete_generation", "gen_1")
    assert (seen[-1].url.path, seen[-1].url.query) == ("/v1/generations/gen_1", b"")
    await call(client, "delete_generation", "gen_1", all=True)
    assert seen[-1].url.query == b"all=true"


async def test_generation_audio_is_downloaded_through_its_url_without_a_signature(make_client):
    def handler(request):
        if request.url.path.startswith("/v1/generations/gen_1/audio"):
            return json_response({"url": "https://blobs.test/gen_1.wav?sig=1", "expires_at": "2026-10-01T13:00:00Z"})
        return httpx.Response(200, content=b"RIFFaudio")

    client, seen = make_client(handler)
    url = await call(client, "generation_audio_url", "gen_1")
    assert seen[-1].url.query == b"redirect=false"
    assert url.url == "https://blobs.test/gen_1.wav?sig=1"
    assert url.expires_at == datetime(2026, 10, 1, 13, tzinfo=timezone.utc)
    await call(client, "generation_audio_url", "gen_1", download=True)
    assert seen[-1].url.query == b"redirect=false&download=true"
    assert await call(client, "download_generation_audio", "gen_1") == b"RIFFaudio"
    assert str(seen[-1].url) == "https://blobs.test/gen_1.wav?sig=1"
    assert "authorization" not in seen[-1].headers


async def test_jobs_are_created_from_text_or_a_script(make_client):
    client, seen = make_client(lambda r: json_response(job("queued"), 202))
    created = await call(
        client, "create_job", JobText("Xin", "haley-f1"),
        model="fp16", format="opus", webhook_url="https://example.com/hook", store_audio=True, idempotency_key="job-1",
    )
    assert created.status == JobStatus.QUEUED
    request = seen[-1]
    assert (request.method, request.url.path) == ("POST", "/v1/jobs")
    assert request.headers["idempotency-key"] == "job-1"
    assert json.loads(request.content) == {
        "text": "Xin", "voice": "haley-f1", "model": "fp16", "response_format": "opus",
        "webhook_url": "https://example.com/hook", "store_audio": True,
    }
    await call(client, "create_job", JobScript([Turn("a", "one"), Turn("b", "two")], gap_ms=250))
    assert json.loads(seen[-1].content) == {
        "turns": [{"voice": "a", "text": "one"}, {"voice": "b", "text": "two"}], "gap_ms": 250,
    }


async def test_jobs_are_read_listed_and_cancelled(make_client):
    def handler(request):
        if request.url.path == "/v1/jobs":
            return json_response({"object": "list", "data": [job("running")], "next_cursor": None})
        return json_response(job("cancelled"))

    client, seen = make_client(handler)
    assert (await call(client, "get_job", "job_1")).progress.characters_done == 5
    page = await call(client, "list_jobs", status=JobStatus.RUNNING, limit=2)
    assert page.data[0].status == JobStatus.RUNNING and page.next_cursor is None
    assert seen[-1].url.query == b"limit=2&status=running"
    assert (await call(client, "cancel_job", "job_1")).status == JobStatus.CANCELLED
    assert (seen[-1].method, seen[-1].url.path) == ("POST", "/v1/jobs/job_1/cancel")


async def test_an_unknown_status_is_kept_instead_of_failing(make_client):
    client, _ = make_client(lambda r: json_response(job("paused")))
    status = (await call(client, "get_job", "job_1")).status
    assert status == "paused" and not status.is_finished


async def test_wait_for_job_polls_until_a_final_state(make_client):
    statuses = iter(["queued", "running", "succeeded", "failed"])
    client, seen = make_client(lambda r: json_response(job(next(statuses))))
    finished = await call(client, "wait_for_job", "job_1", poll_interval=0.005, timeout=5)
    assert finished.status == JobStatus.SUCCEEDED
    assert len(seen) == 3


async def test_wait_for_job_times_out_with_the_last_job(make_client):
    client, _ = make_client(lambda r: json_response(job("running")))
    with pytest.raises(SpeechWaitTimeoutError) as raised:
        await call(client, "wait_for_job", "job_1", poll_interval=0.005, timeout=0.04)
    assert raised.value.last_job.status == JobStatus.RUNNING


async def test_models_and_the_webhook_secret(make_client):
    def handler(request):
        if request.url.path == "/v1/models":
            return json_response({"object": "list", "data": [{"id": "fp16", "profile": "fast"}]})
        return json_response({"secret": "whsec_abc", "created_at": "2026-10-01T12:00:00Z"})

    client, seen = make_client(handler)
    models = await call(client, "list_models")
    assert (models[0].id, models[0].extra["profile"]) == ("fp16", "fast")
    secret = await call(client, "rotate_webhook_secret")
    assert secret.secret == "whsec_abc"
    assert (seen[-1].method, seen[-1].url.path) == ("POST", "/v1/webhook-secret")


async def test_service_errors_carry_their_extra_fields(make_client):
    client, _ = make_client(
        lambda r: json_response(
            {"error": {"code": "IdempotencyReplayed", "message": "already ran", "resource_id": "gen_5", "quota": {"limit": 3}}},
            409,
        )
    )
    with pytest.raises(SpeechServiceError) as raised:
        await call(client, "synthesize", "hi", "haley-f1")
    error = raised.value
    assert (error.status, error.code, error.message, error.resource_id) == (409, "IdempotencyReplayed", "already ran", "gen_5")
    assert error.details == {"quota": {"limit": 3}}


async def test_transport_failures_are_transport_errors(make_client):
    def refuse(request):
        raise httpx.ConnectError("refused")

    client, _ = make_client(refuse)
    with pytest.raises(SpeechTransportError):
        await call(client, "list_voices")


async def test_a_failing_credentials_provider_is_a_signing_error_and_nothing_is_sent():
    def broken():
        raise RuntimeError("sts down")

    for cls in (SpeechClient, AsyncSpeechClient):
        seen = []
        client = cls("https://speech.test", broken, transport=httpx.MockTransport(lambda r: seen.append(r) or httpx.Response(200)))
        with pytest.raises(SpeechSigningError):
            await call(client, "list_voices")
        assert seen == []


async def test_the_provider_is_invalidated_when_the_service_refuses_its_credentials(make_client):
    class Provider:
        invalidated = 0

        def __call__(self):
            return CREDENTIALS

        def invalidate(self):
            self.invalidated += 1

    provider = Provider()
    client, _ = make_client(lambda r: json_response({"error": {"code": "InvalidClientTokenId", "message": "revoked"}}, 403))
    client._sender.credentials = provider
    with pytest.raises(SpeechServiceError):
        await call(client, "list_voices")
    assert provider.invalidated == 1


async def test_line_breaks_cannot_break_the_multipart_framing(make_client):
    client, seen = make_client(lambda r: json_response(VOICE, 201))
    consent = Consent(statement_version=1, speaker_name="An", relationship=Relationship.SELF, agreed=True)
    with pytest.raises(SpeechSigningError):
        await call(client, "clone_voice", "Mine", VoiceReference(b"x", "audio/wav\r\nX: y", "a.wav"), consent)
    assert seen == []
    await call(client, "clone_voice", "Mine", VoiceReference(b"x", "audio/wav", 'a"\r\nb.wav'), consent)
    assert b'filename="a%22%0D%0Ab.wav"' in seen[-1].content
