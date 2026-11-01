// HTTP client for slacksitter-server (see SERVER.md). One function per
// endpoint; names and result types follow the Tauri commands they replaced.
// Every request carries `Authorization: Bearer <api key>`.

import type {
  ChannelSummary,
  ChannelWithMessages,
  MeResult,
  SlackUserProfileInfo,
  SyncStatus,
} from "../types/models";

export interface ServerConnection {
  baseUrl: string;
  apiKey: string;
}

/** `status` is 0 when the server could not be reached at all. */
export class ApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }

  // String(err) is what the status panel and the connection screen show.
  override toString(): string {
    return this.message;
  }
}

export function isUnauthorized(err: unknown): boolean {
  return err instanceof ApiError && err.status === 401;
}

let connection: ServerConnection | null = null;

export function setConnection(next: ServerConnection | null) {
  connection = next ? { baseUrl: normalizeBaseUrl(next.baseUrl), apiKey: next.apiKey.trim() } : null;
}

export function getConnection(): ServerConnection | null {
  return connection;
}

export function normalizeBaseUrl(url: string): string {
  return url.trim().replace(/\/+$/, "");
}

async function request(conn: ServerConnection, path: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers);
  headers.set("Authorization", `Bearer ${conn.apiKey}`);
  if (init.body !== undefined && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  let response: Response;
  try {
    response = await fetch(`${conn.baseUrl}${path}`, { ...init, headers });
  } catch (err) {
    if (err instanceof DOMException && err.name === "AbortError") throw err;
    throw new ApiError(`サーバーに接続できません (${conn.baseUrl})`, 0);
  }

  if (!response.ok) throw new ApiError(await errorMessage(response), response.status);
  return response;
}

async function errorMessage(response: Response): Promise<string> {
  try {
    const body = await response.json();
    if (typeof body?.error === "string") return body.error;
  } catch {
    // Not JSON (a proxy's error page, say); fall through.
  }
  if (response.status === 401) return "API キーが受け付けられませんでした";
  return `サーバーがエラーを返しました (HTTP ${response.status})`;
}

function send(path: string, init?: RequestInit): Promise<Response> {
  if (!connection) return Promise.reject(new ApiError("サーバーに接続していません", 0));
  return request(connection, path, init);
}

async function json<T>(path: string, init?: RequestInit): Promise<T> {
  return (await send(path, init)).json() as Promise<T>;
}

function post<T>(path: string, body?: unknown): Promise<T> {
  return json<T>(path, { method: "POST", body: body === undefined ? undefined : JSON.stringify(body) });
}

// -------------------------------------------------------------- connecting

/**
 * Checks a server URL + API key before they are saved: `/api/health` for
 * reachability (no key needed), then `/api/me` for the key. Does not change
 * the active connection.
 */
export async function testConnection(candidate: ServerConnection): Promise<MeResult> {
  const conn = { baseUrl: normalizeBaseUrl(candidate.baseUrl), apiKey: candidate.apiKey.trim() };
  try {
    const health = await fetch(`${conn.baseUrl}/api/health`);
    if (!health.ok) throw new ApiError(`サーバーが正常に応答しません (HTTP ${health.status})`, health.status);
  } catch (err) {
    if (err instanceof ApiError) throw err;
    throw new ApiError(`サーバーに接続できません (${conn.baseUrl})`, 0);
  }
  return (await request(conn, "/api/me")).json() as Promise<MeResult>;
}

export const getMe = () => json<MeResult>("/api/me");

// ---------------------------------------------------------------- channels

export const listAllChannels = () => json<ChannelSummary[]>("/api/channels");

export const getChannel = (channelId: string) =>
  json<ChannelWithMessages>(`/api/channels/${encodeURIComponent(channelId)}`);

/** Re-reads one channel from the server's database (it does not call Slack). */
export const refreshChannel = getChannel;

const BATCH_SIZE = 50;

/**
 * Reads several channels (unknown ids are skipped). Requests go out in
 * batches; `onBatch` receives each as it arrives so the board can fill progressively.
 */
export async function loadChannels(
  channelIds: string[],
  onBatch?: (channels: ChannelWithMessages[]) => void,
): Promise<ChannelWithMessages[]> {
  const all: ChannelWithMessages[] = [];
  for (let start = 0; start < channelIds.length; start += BATCH_SIZE) {
    const batch = await post<ChannelWithMessages[]>("/api/channels/batch", {
      channelIds: channelIds.slice(start, start + BATCH_SIZE),
    });
    onBatch?.(batch);
    all.push(...batch);
  }
  return all;
}

export const getUserProfile = (userId: string) =>
  json<SlackUserProfileInfo>(`/api/users/${encodeURIComponent(userId)}`);

export const getEmojiMap = () => json<Record<string, string>>("/api/emoji");

// ------------------------------------------------------------ writing back

export const postMessage = (channel: string, text: string) =>
  post<{ ok: boolean; channelId: string }>("/api/messages", { channel, text }).then(() => undefined);

export const toggleReaction = (channelId: string, ts: string, name: string) =>
  post<{ reacted: boolean }>("/api/reactions/toggle", { channelId, ts, name }).then(() => undefined);

// -------------------------------------------------------------------- sync

/** Asks the server to sync one channel now. Completion arrives as `sync-completed`. */
export const syncChannel = (channelId: string) =>
  post<{ started: boolean }>(`/api/sync/channels/${encodeURIComponent(channelId)}`).then(() => undefined);

/** Asks the server to sync every target now. Rejects with status 409 while a pass is running. */
export const syncAll = () => post<{ started: boolean }>("/api/sync").then(() => undefined);

export const getSyncStatus = () => json<SyncStatus>("/api/sync/status");

export const getSyncTargets = () =>
  json<{ channelIds: string[] }>("/api/sync/targets").then((body) => body.channelIds);

/** Replaces the server's custom (non-prefix) sync targets. */
export const putSyncTargets = (channelIds: string[]) =>
  json<{ channelIds: string[] }>("/api/sync/targets", {
    method: "PUT",
    body: JSON.stringify({ channelIds }),
  }).then((body) => body.channelIds);

export async function getServerLogs(lines = 500): Promise<string> {
  return (await send(`/api/logs?lines=${lines}`)).text();
}

/** Opens `/api/events`. The caller reads `response.body`. */
export const openEventStream = (signal: AbortSignal) =>
  send("/api/events", { headers: { Accept: "text/event-stream" }, signal });

// ------------------------------------------------------------------ images

// <img src> cannot carry the Authorization header, so images are fetched here
// and shown through object URLs. An object URL is revoked once nothing uses it.
const objectUrlUsers = new Map<string, number>();

/** Fetches the first candidate the server can download and returns an object URL for it. */
export async function fetchImage(candidateUrls: string[]): Promise<string> {
  let lastError: unknown = new ApiError("画像の URL がありません", 0);
  for (const url of candidateUrls) {
    try {
      const response = await send(`/api/images?url=${encodeURIComponent(url)}`);
      const objectUrl = URL.createObjectURL(await response.blob());
      objectUrlUsers.set(objectUrl, 1);
      return objectUrl;
    } catch (err) {
      lastError = err;
      if (isUnauthorized(err) || (err instanceof ApiError && err.status === 0)) break;
    }
  }
  throw lastError;
}

/** Marks one more user of an object URL from `fetchImage` (e.g. the full-size preview). */
export function retainObjectUrl(url: string) {
  const users = objectUrlUsers.get(url);
  if (users !== undefined) objectUrlUsers.set(url, users + 1);
}

/** Releases one user; the object URL is revoked when the last one lets go. */
export function releaseObjectUrl(url: string) {
  const users = objectUrlUsers.get(url);
  if (users === undefined) return;
  if (users <= 1) {
    objectUrlUsers.delete(url);
    URL.revokeObjectURL(url);
  } else {
    objectUrlUsers.set(url, users - 1);
  }
}
