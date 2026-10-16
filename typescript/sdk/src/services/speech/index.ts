export { SpeechClient, type SpeechClientOptions } from "./client.js";
export {
  SpeechError,
  SpeechServiceError,
  SpeechSigningError,
  SpeechTransportError,
  SpeechWaitTimeoutError,
} from "./error.js";
export {
  verifyWebhook,
  verifyWebhookSignature,
  WebhookVerificationError,
  type VerifyWebhookOptions,
  type WebhookFailure,
} from "./webhook.js";
export type * from "./dto.js";
export type * from "./enums.js";
export type { Page } from "../../common/pagination.js";
