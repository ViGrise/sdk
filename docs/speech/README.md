# Speech

Text to speech with built-in and cloned voices: synthesis (whole or streamed),
voice cloning with recorded consent, generation history, background jobs with
webhooks, and model listing. The endpoint is `https://speech-api.vigrise.com`
unless [overridden](../auth/README.md#endpoints); every call is VGR4-signed for
the `speech` service.

| | Rust | TypeScript | Python |
| --- | --- | --- | --- |
| Package | `vigrise`, feature `speech` | `@vigrise/sdk/speech` | `vigrise.speech` (extra `speech`) |
| Client | `SpeechClient` | `SpeechClient` | `SpeechClient`, `AsyncSpeechClient` |
| Runs in | Tokio | Node 24 and browsers | Python 3.9+ |

## Synthesize

```rust
use std::sync::Arc;
use vigrise::auth::Credentials;
use vigrise::speech::{SpeechClient, SynthesizeOptions};

let speech = SpeechClient::new(Arc::new(Credentials::resolve()?));
let spoken = speech.synthesize("Xin chào", "haley-f1", &SynthesizeOptions::default()).await?;
std::fs::write("hello.wav", &spoken.audio)?;
```

```ts
import { SpeechClient } from "@vigrise/sdk/speech";

const speech = new SpeechClient();
const { audio, contentType } = await speech.synthesize("Xin chào", "haley-f1", { format: "mp3" });
```

```python
from vigrise.speech import SpeechClient

with SpeechClient() as speech:
    spoken = speech.synthesize("Xin chào", "haley-f1", format="mp3")
    open("hello.mp3", "wb").write(spoken.audio)
```

`synthesize_stream` (`synthesizeStream`) returns the audio as it is produced: a
`Stream` in Rust, a `ReadableStream` in TypeScript, an iterator in Python (an
async iterator from `AsyncSpeechClient`).

## Jobs and webhooks

A job synthesizes a long text or a script of turns in the background and can
report to a webhook.

```rust
use vigrise::speech::{JobInput, JobOptions, WaitOptions};

let job = speech
    .create_job(
        &JobInput::Text { text: "A long text".into(), voice: "haley-f1".into() },
        &JobOptions { webhook_url: Some(hook_url), ..Default::default() },
    )
    .await?;
let done = speech.wait_for_job(&job.id, &WaitOptions::default()).await?;
let audio = speech.download_generation_audio(&done.generation_id.unwrap()).await?;

// In the webhook handler, with the raw request body:
let event = vigrise::speech::verify_webhook(&secret, &headers, &raw_body)?;
```

```ts
const job = await speech.createJob({ text: "A long text", voice: "haley-f1" }, { webhookUrl });
const done = await speech.waitForJob(job.id, { timeoutMs: 60_000 });
const audio = await speech.downloadGenerationAudio(done.generationId!);

import { verifyWebhook } from "@vigrise/sdk/speech";
const event = await verifyWebhook(secret, request.headers, rawBody);
```

```python
from vigrise.speech import JobText, verify_webhook

job = speech.create_job(JobText("A long text", "haley-f1"), webhook_url=webhook_url)
done = speech.wait_for_job(job.id, timeout=60)
audio = speech.download_generation_audio(done.generation_id)

event = verify_webhook(secret, request.headers, raw_body)
```

`rotate_webhook_secret` creates or replaces the project's secret
(`whsec_` followed by base64). Verification follows Standard Webhooks and needs
no credentials or network: it checks `webhook-id`, `webhook-timestamp` and
`webhook-signature` against the raw body, refuses timestamps more than five
minutes from the clock, and returns the event (`job.succeeded`, `job.failed` or
`job.cancelled` with the job).

## Voices, generations and models

| Operation | Rust | TypeScript | Python |
| --- | --- | --- | --- |
| Voices | `list_voices`, `get_voice`, `delete_voice`, `clone_voice`, `consent_statement` | `listVoices`, `getVoice`, `deleteVoice`, `cloneVoice`, `consentStatement` | `list_voices`, `get_voice`, `delete_voice`, `clone_voice`, `consent_statement` |
| History | `list_generations`, `get_generation`, `delete_generation`, `generation_audio_url`, `download_generation_audio` | camelCase of the same | snake_case of the same |
| Jobs | `create_job`, `get_job`, `list_jobs`, `cancel_job`, `wait_for_job` | camelCase | snake_case |
| Models | `list_models` | `listModels` | `list_models` |

Cloning needs the speaker's consent: read `consent_statement`, show it, and send
the version with `speaker_name`, `relationship` (`self` or `authorized`) and
`agreed`.

## Errors and retries

Service errors carry the HTTP status, the `code` and `message` of the error
envelope, and its extra fields: `resource_id` (the generation an idempotency key
already created) and `quota` details on limit refusals. Transport, signing and
timeout failures are separate error types. Nothing is retried automatically;
pass `idempotency_key` (`idempotencyKey`) to repeat a call safely.

## Examples

`rust/vigrise/examples/speech.rs`, `typescript/sdk/examples/speech.mjs` and
`python/examples/speech.py` list voices or synthesize; the `speech_tour`
examples of each language run a synthesis, a job with a webhook and the audio
download.
