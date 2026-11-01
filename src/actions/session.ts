// Shared imperative actions used by several components (app shell, controller
// bar, user popup). They read/write the Zustand stores directly via getState()
// so they can be called from event handlers and timers alike.
//
// Slack is read by the server, not by this app: these actions only load what
// the server has already stored, ask it to sync, and react to its SSE events.

import {
  getEmojiMap,
  getSyncStatus,
  isUnauthorized,
  listAllChannels,
  loadChannels,
  setConnection,
  syncChannel,
  getChannel,
} from "../api/client";
import { clearServerSettings, writeLog } from "../api/commands";
import { connectEvents, type ServerEvent } from "../api/events";
import { useAuthStore } from "../store/authStore";
import { useBoardStore } from "../store/boardStore";
import { useChannelsStore } from "../store/channelsStore";
import { useEmojiStore } from "../store/emojiStore";
import { useUiStore } from "../store/uiStore";
import { selectVisibleChannels } from "../store/visibleChannels";
import { customBoardChannelIds } from "./boards";

/** While the event stream is down the board is refreshed this often instead. */
const POLL_INTERVAL_MS = 30_000;
/** The refresh button stops spinning after this even if the server never reports completion. */
const REFRESH_TIMEOUT_MS = 60_000;
/** `channel-updated` events arriving together are read in one go. */
const CHANNEL_RELOAD_DELAY_MS = 200;

const UNAUTHORIZED_MESSAGE = "API キーが受け付けられませんでした。接続設定を確認してください";

// ------------------------------------------------------------- loading data

let workspaceLoading = false;

/**
 * Loads what the board needs from the server's database: the emoji, the
 * channel catalog, then the prefix channels plus the channels custom boards
 * use (in batches, so the board fills progressively). Reads only; Slack is
 * synced by the server on its own schedule.
 */
export async function loadWorkspace(options: { showLoading?: boolean } = {}) {
  if (workspaceLoading) return;
  workspaceLoading = true;

  const ui = useUiStore.getState();
  ui.setRefreshing(true);
  if (options.showLoading) ui.setStatus("チャンネルを読み込んでいます...", "loading");

  try {
    const [catalog] = await Promise.all([listAllChannels(), reloadEmoji()]);
    const prefix = useAuthStore.getState().channelPrefix;
    const ids = Array.from(
      new Set([
        ...catalog.filter((channel) => channel.name.startsWith(prefix)).map((channel) => channel.channelId),
        ...customBoardChannelIds(),
      ]),
    );
    await loadChannels(ids, (batch) => useChannelsStore.getState().upsertMany(batch));

    useUiStore.getState().setServerReachable(true);
    clearStatusIf("loading");
  } catch (err) {
    handleRequestError(err, "workspace load failed");
  } finally {
    useUiStore.getState().setRefreshing(false);
    workspaceLoading = false;
  }
}

async function reloadEmoji() {
  try {
    useEmojiStore.getState().setEmojiMap(await getEmojiMap());
  } catch {
    // Missing custom emoji only degrades to ":name:" text.
  }
}

/** Re-reads every channel already in the store (after a reconnect, or while polling). */
export async function reloadStoredChannels() {
  const ids = Array.from(useChannelsStore.getState().channels.keys());
  if (ids.length === 0) return;
  await loadChannels(ids, (batch) => useChannelsStore.getState().upsertMany(batch));
}

function clearStatusIf(kind: "loading" | "warning" | "error") {
  const ui = useUiStore.getState();
  if (ui.statusKind === kind) ui.setStatus(null, null);
}

/** Shows a failed request in the status panel; a rejected API key goes back to the connection screen. */
function handleRequestError(err: unknown, context: string) {
  if (isUnauthorized(err)) {
    handleUnauthorized();
    return;
  }
  void writeLog("error", `${context}: ${String(err)}`);
  useUiStore.getState().setStatus(String(err), "error");
}

function handleUnauthorized() {
  void writeLog("warn", "the server rejected the API key");
  teardownSession();
  useAuthStore.getState().setLoggedOut(UNAUTHORIZED_MESSAGE);
}

// -------------------------------------------------------- the refresh button

let refreshTimer: number | undefined;
/** Channel syncs requested by the refresh button that have not reported `sync-completed` yet. */
let pendingSyncCompletions = 0;

function finishRefresh() {
  window.clearTimeout(refreshTimer);
  refreshTimer = undefined;
  pendingSyncCompletions = 0;
  useUiStore.getState().setRefreshing(false);
}

/**
 * The refresh button: asks the server to sync the channels on the board being
 * shown right now (instead of waiting for its next round). Each request ends
 * with a `sync-completed` event (or a timeout), which stops the spinner; the new
 * content arrives through `channel-updated`.
 * With nothing on screen yet, it falls back to loading from the server.
 */
export async function refreshActiveBoard() {
  const ui = useUiStore.getState();
  if (ui.isRefreshing) return;

  const boards = useBoardStore.getState();
  const visible = selectVisibleChannels(
    useChannelsStore.getState().channels,
    ui.activeFilter,
    boards.customBoards,
    boards.activeCustomBoardIndex,
    useAuthStore.getState().channelPrefix,
  );
  if (visible.length === 0) {
    await loadWorkspace({ showLoading: true });
    return;
  }

  ui.setRefreshing(true);
  pendingSyncCompletions = visible.length;
  refreshTimer = window.setTimeout(finishRefresh, REFRESH_TIMEOUT_MS);
  try {
    await Promise.all(visible.map((channel) => syncChannel(channel.channelId)));
  } catch (err) {
    finishRefresh();
    handleRequestError(err, "sync request failed");
  }
}

// ------------------------------------------------------------ server events

let syncErrorShown = false;
let reloadTimer: number | undefined;
const channelsToReload = new Set<string>();

function queueChannelReload(channelId: string) {
  channelsToReload.add(channelId);
  if (reloadTimer !== undefined) return;
  reloadTimer = window.setTimeout(() => {
    reloadTimer = undefined;
    const ids = Array.from(channelsToReload);
    channelsToReload.clear();
    const load = ids.length === 1 ? getChannel(ids[0]).then((channel) => [channel]) : loadChannels(ids);
    load
      .then((channels) => useChannelsStore.getState().upsertMany(channels))
      .catch((err) => void writeLog("warn", `could not reload updated channels: ${String(err)}`));
  }, CHANNEL_RELOAD_DELAY_MS);
}

function handleServerEvent(event: ServerEvent) {
  switch (event.type) {
    case "channel-updated":
      queueChannelReload(event.channelId);
      break;
    case "sync-completed":
      if (pendingSyncCompletions > 0 && --pendingSyncCompletions === 0) finishRefresh();
      if (event.failed === 0 && syncErrorShown) {
        syncErrorShown = false;
        clearStatusIf("error");
      }
      break;
    case "sync-error":
      // The server passes Slack's error code through (e.g. "missing_scope"); the
      // status panel shows the needed scopes when the message mentions one.
      syncErrorShown = true;
      useUiStore.getState().setStatus(event.message, "error");
      void writeLog("warn", `sync error shown to the user: ${event.message}`);
      break;
    default:
      break;
  }
}

// ------------------------------------------------------------ the live link

let stopEvents: (() => void) | undefined;
let pollTimer: number | undefined;

function stopPolling() {
  window.clearInterval(pollTimer);
  pollTimer = undefined;
}

function startPolling() {
  if (pollTimer !== undefined) return;
  pollTimer = window.setInterval(() => void pollOnce(), POLL_INTERVAL_MS);
}

/** One round of the fallback poll: is the server back, and has anything changed? */
async function pollOnce() {
  try {
    await getSyncStatus();
    useUiStore.getState().setServerReachable(true);
    await reloadStoredChannels();
  } catch (err) {
    if (isUnauthorized(err)) handleUnauthorized();
    else useUiStore.getState().setServerReachable(false);
  }
}

/**
 * Starts listening to the server (SSE). While the stream is down the server is
 * polled every 30 seconds instead, and the status panel says it is unreachable.
 * When the stream comes back the board is re-read to catch up.
 */
export function startServerSession() {
  stopSession();
  let everConnected = false;
  stopEvents = connectEvents({
    onOpen: () => {
      stopPolling();
      useUiStore.getState().setServerReachable(true);
      if (everConnected) {
        void reloadEmoji();
        void reloadStoredChannels().catch(() => {});
      }
      everConnected = true;
    },
    onClose: () => {
      useUiStore.getState().setServerReachable(false);
      startPolling();
    },
    onEvent: handleServerEvent,
    onUnauthorized: handleUnauthorized,
  });
  return stopSession;
}

/** Stops listening to the server and drops what was queued from it. */
function stopSession() {
  stopEvents?.();
  stopEvents = undefined;
  stopPolling();
  window.clearTimeout(reloadTimer);
  reloadTimer = undefined;
  channelsToReload.clear();
  syncErrorShown = false;
}

/** stopSession, plus resetting the UI state that belonged to the connection. */
function teardownSession() {
  stopSession();
  finishRefresh();
  useUiStore.getState().setServerReachable(true);
}

// ------------------------------------------------------------------ logout

/** Forgets the saved server URL / API key and returns to the connection screen. */
export async function performLogout() {
  teardownSession();
  setConnection(null);
  await clearServerSettings().catch((err) => void writeLog("error", `could not clear server settings: ${String(err)}`));
  const ui = useUiStore.getState();
  ui.setOpenPopup(null);
  ui.setStatus(null, null);
  useChannelsStore.getState().clear();
  useAuthStore.getState().setLoggedOut();
}
