export { canonicalRequest, canonicalQuery, canonicalUri, sha256Hex } from "./canonical.js";
export { parseProfile, resolveCredentials, type Credentials, type ResolveOptions } from "./credentials.js";
export { signedFetch, type SignedFetchOptions } from "./fetch.js";
export { signRequest, type SignInput, type SignedHeaders } from "./signer.js";
export { ALGORITHM, TERMINATOR, deriveSigningKey, formatTimestamp, scope, signature, stringToSign } from "./signing.js";
