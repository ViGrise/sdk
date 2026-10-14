export { DEFAULT_ENDPOINT, SpeechClient, type SpeechClientOptions } from "./client.js";
export { SpeechError, SpeechServiceError, SpeechSigningError, SpeechTransportError, SpeechWaitTimeoutError } from "./errors.js";
export { verifyWebhook, verifyWebhookSignature, WebhookVerificationError, type VerifyWebhookOptions, type WebhookFailure } from "./webhook.js";
export type * from "./types.js";
