// Shared imperative actions used by several components (app shell, controller
// bar, user popup). They read/write the Zustand stores directly via getState()
// so they can be called from event handlers and timers alike.

import { logout, refreshWorkspaceData } from "../api/commands";
import { useAuthStore } from "../store/authStore";
import { useChannelsStore } from "../store/channelsStore";
import { useUiStore } from "../store/uiStore";

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

  refreshWorkspaceData().catch((err) => {
    const current = useUiStore.getState();
    current.setRefreshing(false);
    current.setStatus(String(err), "error");
  });
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
