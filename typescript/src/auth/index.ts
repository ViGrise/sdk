export { envKey, resolveEndpoint, stsBaseUrl, stsUrl } from "../common/endpoint.js";
export { canonicalRequest, canonicalQuery, canonicalUri, sha256Hex } from "./canonical.js";
export { parseProfile, resolveCredentials, type Credentials, type ResolveOptions } from "./credentials.js";
export { signedFetch, type SignedFetchOptions } from "./fetch.js";
export { UNSIGNED_PAYLOAD, signRequest, type SignInput, type SignedHeaders } from "./signer.js";
export { ALGORITHM, TERMINATOR, deriveSigningKey, formatTimestamp, scope, signature, stringToSign } from "./signing.js";
export { assumeRole, type AssumeRoleInput, type AssumeRoleOptions, type RoleSession } from "./sts.js";
export type { CredentialsProvider } from "./types.js";
