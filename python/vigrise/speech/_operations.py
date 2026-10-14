"""What each Speech operation sends and how it reads the answer. The sync and async clients share these."""

import json
from typing import Any, Dict, List, Optional

import httpx

from .._http import Request, query_pairs, segment
from ._errors import SpeechTransportError, error_from_response
from ._models import (
    AudioFormat,
    AudioUrl,
    Consent,
    ConsentStatement,
    Generation,
    GenerationSource,
    Job,
    JobInput,
    JobScript,
    JobStatus,
    Model,
    Page,
    Speech,
    Voice,
    VoiceReference,
    WebhookSecret,
    _time,
)
from ._multipart import encode_multipart

_JSON = {"content-type": "application/json"}


def _json_request(method: str, path: str, body: Dict[str, Any], idempotency_key: Optional[str] = None) -> Request:
    headers = dict(_JSON)
    if idempotency_key is not None:
        headers["idempotency-key"] = idempotency_key
    return Request(method, path, headers=headers, content=json.dumps(body, ensure_ascii=False, separators=(",", ":")).encode())


def _without_none(fields: Dict[str, Any]) -> Dict[str, Any]:
    return {name: value for name, value in fields.items() if value is not None}


def check(response: httpx.Response) -> httpx.Response:
    """The response when it succeeded; the service error it carries otherwise."""
    if response.is_success:
        return response
    raise error_from_response(response.status_code, response.read())


def read_json(response: httpx.Response) -> Any:
    try:
        return response.json()
    except ValueError as error:
        raise SpeechTransportError(f"the answer is not JSON: {error}") from error


def synthesize_request(text: str, voice: str, stream: bool, model: Optional[str], format: Optional[AudioFormat],
                       store_audio: Optional[bool], idempotency_key: Optional[str]) -> Request:
    body = _without_none({
        "input": text,
        "voice": voice,
        "model": model,
        "response_format": None if format is None else str(format),
        "store_audio": store_audio,
        "stream": True if stream else None,
    })
    return _json_request("POST", "/v1/audio/speech", body, idempotency_key)


def audio_metadata(response: httpx.Response):
    rate = response.headers.get("x-audio-sample-rate")
    return (
        response.headers.get("content-type", ""),
        response.headers.get("x-vgr-generation-id", ""),
        int(rate) if rate is not None and rate.isdigit() else None,
    )


def parse_speech(response: httpx.Response) -> Speech:
    content_type, generation_id, sample_rate = audio_metadata(response)
    return Speech(response.content, content_type, generation_id, sample_rate)


def list_voices_request() -> Request:
    return Request("GET", "/v1/voices")


def parse_voices(response: httpx.Response) -> List[Voice]:
    return [Voice.from_dict(item) for item in read_json(response)["data"]]


def get_voice_request(voice_id: str) -> Request:
    return Request("GET", f"/v1/voices/{segment(voice_id)}")


def parse_voice(response: httpx.Response) -> Voice:
    return Voice.from_dict(read_json(response))


def delete_voice_request(voice_id: str) -> Request:
    return Request("DELETE", f"/v1/voices/{segment(voice_id)}")


def clone_voice_request(name: str, reference: VoiceReference, consent: Consent, idempotency_key: Optional[str]) -> Request:
    content_type, body = encode_multipart([
        ("name", name.encode(), None, None),
        ("reference", reference.bytes, reference.file_name, reference.content_type),
        ("consent", json.dumps(consent.to_dict(), separators=(",", ":")).encode(), None, None),
    ])
    headers = {"content-type": content_type}
    if idempotency_key is not None:
        headers["idempotency-key"] = idempotency_key
    return Request("POST", "/v1/voices", headers=headers, content=body)


def consent_statement_request() -> Request:
    return Request("GET", "/v1/consent-statement")


def parse_consent_statement(response: httpx.Response) -> ConsentStatement:
    data = read_json(response)
    return ConsentStatement(data["version"], data["statement"])


def list_generations_request(cursor: Optional[str], limit: Optional[int], source: Optional[GenerationSource], voice: Optional[str]) -> Request:
    return Request("GET", "/v1/generations", query=query_pairs(cursor=cursor, limit=limit, source=source, voice=voice))


def parse_generation_page(response: httpx.Response) -> Page[Generation]:
    data = read_json(response)
    return Page([Generation.from_dict(item) for item in data["data"]], data.get("next_cursor"))


def get_generation_request(generation_id: str) -> Request:
    return Request("GET", f"/v1/generations/{segment(generation_id)}")


def parse_generation(response: httpx.Response) -> Generation:
    return Generation.from_dict(read_json(response))


def delete_generation_request(generation_id: str, all: bool) -> Request:
    return Request("DELETE", f"/v1/generations/{segment(generation_id)}", query=query_pairs(all=True if all else None))


def generation_audio_url_request(generation_id: str, download: bool) -> Request:
    return Request(
        "GET",
        f"/v1/generations/{segment(generation_id)}/audio",
        query=query_pairs(redirect=False, download=True if download else None),
    )


def parse_audio_url(response: httpx.Response) -> AudioUrl:
    data = read_json(response)
    return AudioUrl(data["url"], _time(data["expires_at"]))


def create_job_request(input: JobInput, model: Optional[str], format: Optional[AudioFormat], webhook_url: Optional[str],
                       store_audio: Optional[bool], idempotency_key: Optional[str]) -> Request:
    if isinstance(input, JobScript):
        content = _without_none({"turns": [{"voice": turn.voice, "text": turn.text} for turn in input.turns], "gap_ms": input.gap_ms})
    else:
        content = {"text": input.text, "voice": input.voice}
    body = dict(content, **_without_none({
        "model": model,
        "response_format": None if format is None else str(format),
        "webhook_url": webhook_url,
        "store_audio": store_audio,
    }))
    return _json_request("POST", "/v1/jobs", body, idempotency_key)


def get_job_request(job_id: str) -> Request:
    return Request("GET", f"/v1/jobs/{segment(job_id)}")


def parse_job(response: httpx.Response) -> Job:
    return Job.from_dict(read_json(response))


def list_jobs_request(cursor: Optional[str], limit: Optional[int], status: Optional[JobStatus]) -> Request:
    return Request("GET", "/v1/jobs", query=query_pairs(cursor=cursor, limit=limit, status=status))


def parse_job_page(response: httpx.Response) -> Page[Job]:
    data = read_json(response)
    return Page([Job.from_dict(item) for item in data["data"]], data.get("next_cursor"))


def cancel_job_request(job_id: str) -> Request:
    return Request("POST", f"/v1/jobs/{segment(job_id)}/cancel")


def list_models_request() -> Request:
    return Request("GET", "/v1/models")


def parse_models(response: httpx.Response) -> List[Model]:
    return [Model(item["id"], {name: value for name, value in item.items() if name != "id"}) for item in read_json(response)["data"]]


def rotate_webhook_secret_request() -> Request:
    return Request("POST", "/v1/webhook-secret")


def parse_webhook_secret(response: httpx.Response) -> WebhookSecret:
    data = read_json(response)
    return WebhookSecret(data["secret"], _time(data["created_at"]))
