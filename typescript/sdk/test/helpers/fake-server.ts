import { createServer, type IncomingHttpHeaders, type Server } from "node:http";
import type { AddressInfo } from "node:net";

export interface SeenRequest {
  method: string;
  target: string;
  headers: IncomingHttpHeaders;
  body: Buffer;
  json(): any;
  text(): string;
}

export interface FakeReply {
  status?: number;
  headers?: Record<string, string>;
  /** A body, or chunks that are sent one after the other. */
  body?: string | Uint8Array | Iterable<string | Uint8Array>;
}

export interface FakeServer {
  url: string;
  requests: SeenRequest[];
  last(): SeenRequest;
  close(): Promise<void>;
}

/** Starts a local HTTP server that records every request and answers with `respond`. */
export async function fakeServer(
  respond: (request: SeenRequest) => FakeReply | Promise<FakeReply>,
): Promise<FakeServer> {
  const requests: SeenRequest[] = [];
  const server: Server = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk) => chunks.push(chunk));
    req.on("end", async () => {
      const body = Buffer.concat(chunks);
      const seen: SeenRequest = {
        method: req.method ?? "",
        target: req.url ?? "",
        headers: req.headers,
        body,
        json: () => JSON.parse(body.toString("utf8")),
        text: () => body.toString("utf8"),
      };
      requests.push(seen);
      const reply = await respond(seen);
      res.writeHead(reply.status ?? 200, reply.headers ?? {});
      const parts =
        typeof reply.body === "string" || reply.body instanceof Uint8Array ? [reply.body] : [...(reply.body ?? [])];
      for (const part of parts) {
        res.write(part);
        await new Promise((resolve) => setTimeout(resolve, 2));
      }
      res.end();
    });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address() as AddressInfo;
  return {
    url: `http://127.0.0.1:${port}`,
    requests,
    last: () => requests[requests.length - 1]!,
    close: () =>
      new Promise<void>((resolve) => {
        server.closeAllConnections();
        server.close(() => resolve());
      }),
  };
}

export function jsonReply(body: unknown, status = 200): FakeReply {
  return { status, headers: { "content-type": "application/json" }, body: JSON.stringify(body) };
}
