// Shared imperative actions used by several components (app shell, controller
// bar, user popup). They read/write the Zustand stores directly via getState()
// so they can be called from event handlers and timers alike.

import { loadChannels, logout, refreshWorkspaceData, writeLog } from "../api/commands";
import { useAuthStore } from "../store/authStore";
import { useBoardStore } from "../store/boardStore";
import { useChannelsStore } from "../store/channelsStore";
import { useUiStore } from "../store/uiStore";
import { selectVisibleChannels } from "../store/visibleChannels";
import { customBoardChannelIds } from "./boards";

/**
 * Kicks off `refresh_workspace_data`. Results stream back through the
 * `channel-batch` / `refresh-complete` / `refresh-error` events (see App.tsx),
 * so this only starts the job. A refresh already in flight is never doubled up.
 */
export function startWorkspaceRefresh(options: { showLoading?: boolean } = {}) {
  const ui = useUiStore.getState();
  if (ui.isRefreshing) return;

  ui.setRefreshing(true);
  if (options.showLoading) {
    ui.setStatus("チャンネルを読み込んでいます...", "loading");
  }

  refreshWorkspaceData(customBoardChannelIds()).catch((err) => {
    const current = useUiStore.getState();
    current.setRefreshing(false);
    current.setStatus(String(err), "error");
  });
}

/**
 * The refresh button: reloads just the channels on the board being shown
 * (full content, not a delta, so reactions and thread replies stay current).
 * Falls back to a full workspace refresh when there is nothing to reload yet
 * or the backend could not load any of them (e.g. no channel catalog).
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
  );
  if (visible.length === 0) {
    startWorkspaceRefresh({ showLoading: true });
    return;
  }

  ui.setRefreshing(true);
  let needsFullRefresh = false;
  try {
    const loaded = await loadChannels(visible.map((channel) => channel.channelId));
    if (loaded.length === 0) needsFullRefresh = true;
    else useChannelsStore.getState().upsertMany(loaded);
  } catch (err) {
    void writeLog("error", `active board refresh failed: ${String(err)}`);
    useUiStore.getState().setStatus(String(err), "error");
  } finally {
    useUiStore.getState().setRefreshing(false);
  }

  // After the flag is cleared, so the full refresh can claim it for itself.
  if (needsFullRefresh) startWorkspaceRefresh({ showLoading: true });
}

export async function performLogout() {
  await logout();
  const ui = useUiStore.getState();
  ui.setOpenPopup(null);
  ui.setRefreshing(false);
  ui.setStatus(null, null);
  useChannelsStore.getState().clear();
  useAuthStore.getState().setLoggedOut();
}
