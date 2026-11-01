// Subscribes to the server's `/api/events` stream (SSE) with fetch(), because
// EventSource cannot send the API key header. Reconnects with exponential
// backoff; the caller decides what to do while the stream is down (polling).

import { isUnauthorized, openEventStream } from "./client";
import { SseParser } from "./sse";

/** Who started a sync batch: the server's own rounds, or a request from a client. */
export type SyncKind = "scheduler" | "manual";

export type ServerEvent =
  | { type: "sync-started"; batchId: string; kind: SyncKind; total: number }
  | { type: "sync-progress"; batchId: string; kind: SyncKind; done: number; total: number }
  | { type: "channel-updated"; channelId: string }
  | { type: "sync-completed"; batchId: string; kind: SyncKind; failed: number }
  | { type: "sync-error"; message: string };

export interface EventHandlers {
  /** The server sent its `ready` event: the stream is live. */
  onOpen: () => void;
  /** The stream could not be opened or ended. Reconnecting is already scheduled. */
  onClose: () => void;
  onEvent: (event: ServerEvent) => void;
  /** The server rejected the API key (HTTP 401). No more retries. */
  onUnauthorized: () => void;
}

const INITIAL_BACKOFF_MS = 1000;
const MAX_BACKOFF_MS = 30_000;

/** Delay before reconnect attempt number `attempt` (0-based): 1s, 2s, 4s ... capped at 30s. */
export function backoffDelay(attempt: number): number {
  return Math.min(MAX_BACKOFF_MS, INITIAL_BACKOFF_MS * 2 ** attempt);
}

/** Parses one SSE message into a typed event; unknown or malformed ones give null. */
export function toServerEvent(name: string, data: string): ServerEvent | null {
  let payload: Record<string, unknown>;
  try {
    payload = JSON.parse(data);
  } catch {
    return null;
  }
  const batchId = String(payload.batchId ?? "");
  const kind: SyncKind = payload.kind === "manual" ? "manual" : "scheduler";
  switch (name) {
    case "sync-started":
      return { type: name, batchId, kind, total: Number(payload.total ?? 0) };
    case "sync-progress":
      return { type: name, batchId, kind, done: Number(payload.done ?? 0), total: Number(payload.total ?? 0) };
    case "channel-updated":
      return typeof payload.channelId === "string" ? { type: name, channelId: payload.channelId } : null;
    case "sync-completed":
      return { type: name, batchId, kind, failed: Number(payload.failed ?? 0) };
    case "sync-error":
      return { type: name, message: String(payload.message ?? "") };
    default:
      return null;
  }
}

/** Starts the connect/read/reconnect loop. Returns a function that stops it. */
export function connectEvents(handlers: EventHandlers): () => void {
  let stopped = false;
  const controller = new AbortController();

  const sleep = (ms: number) =>
    new Promise<void>((resolve) => {
      const timer = window.setTimeout(resolve, ms);
      controller.signal.addEventListener("abort", () => {
        window.clearTimeout(timer);
        resolve();
      });
    });

  async function loop() {
    let attempt = 0;
    while (!stopped) {
      try {
        const response = await openEventStream(controller.signal);
        if (!response.body) throw new Error("the event stream has no body");
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        const parser = new SseParser();

        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          for (const message of parser.push(decoder.decode(value, { stream: true }))) {
            if (message.event === "ready") {
              attempt = 0;
              handlers.onOpen();
              continue;
            }
            const event = toServerEvent(message.event, message.data);
            if (event) handlers.onEvent(event);
          }
        }
      } catch (err) {
        if (stopped) return;
        if (isUnauthorized(err)) {
          handlers.onUnauthorized();
          return;
        }
      }
      if (stopped) return;

      handlers.onClose();
      await sleep(backoffDelay(attempt));
      attempt += 1;
    }
  }

  void loop();

  return () => {
    stopped = true;
    controller.abort();
  };
}
