/** Message the tabs of an app exchange when the signed-in account changes. */
export interface SessionMessage {
  type: "session-changed";
}

export interface SessionChannel {
  notify(message: SessionMessage): void;
  subscribe(handler: (message: SessionMessage) => void): () => void;
  close(): void;
}

const NOOP_CHANNEL: SessionChannel = {
  notify() {},
  subscribe: () => () => {},
  close() {},
};

/** A `BroadcastChannel` for session changes; a no-op where the API is missing. */
export function createSessionChannel(name = "vigrise-accounts-session"): SessionChannel {
  if (typeof BroadcastChannel === "undefined") return NOOP_CHANNEL;
  const channel = new BroadcastChannel(name);
  return {
    notify: (message) => channel.postMessage(message),
    subscribe(handler) {
      const listener = (event: MessageEvent<SessionMessage>) => handler(event.data);
      channel.addEventListener("message", listener);
      return () => channel.removeEventListener("message", listener);
    },
    close: () => channel.close(),
  };
}

/** Runs `reload` in this tab whenever another tab announces a session change. */
export function reloadOnSessionChange(
  channel: SessionChannel,
  reload: () => void = () => globalThis.location.reload(),
): () => void {
  return channel.subscribe((message) => {
    if (message.type === "session-changed") reload();
  });
}
