/** A value this version of the SDK may not know yet is still a string. */
export type Open<T extends string> = T | (string & {});

export type AudioFormat = Open<"wav" | "pcm" | "mp3" | "opus">;
export type GenerationSource = Open<"sync" | "stream" | "job">;
export type GenerationStatus = Open<"complete" | "incomplete">;
export type JobStatus = Open<"queued" | "running" | "succeeded" | "failed" | "cancelled">;
export type VoiceType = Open<"system" | "cloned">;
export type Relationship = "self" | "authorized";
