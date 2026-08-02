import { logout, refreshWorkspaceData } from "../api/commands";
import { useAuthStore } from "../store/authStore";
import { useChannelsStore } from "../store/channelsStore";
import { useUiStore } from "../store/uiStore";
import type { BoardFilter } from "../types/models";

const FILTERS: { key: BoardFilter; label: string }[] = [
  { key: "JoinedOnly", label: "参加中" },
  { key: "NotJoinedOnly", label: "未参加" },
  { key: "All", label: "すべて" },
];

/** Simplified stand-in for the legacy bottom-center MainControllerView.
 * The full circle/bubble popup bar (log viewer, custom boards, compose,
 * gear/plus editors) lands in later milestones; this covers M1's read-only
 * board-switch + refresh + logout needs. */
export function TopBar() {
  const activeFilter = useUiStore((s) => s.activeFilter);
  const setActiveFilter = useUiStore((s) => s.setActiveFilter);
  const isRefreshing = useUiStore((s) => s.isRefreshing);
  const setRefreshing = useUiStore((s) => s.setRefreshing);
  const setStatus = useUiStore((s) => s.setStatus);
  const setLoggedOut = useAuthStore((s) => s.setLoggedOut);
  const clearChannels = useChannelsStore((s) => s.clear);

  async function handleRefresh() {
    setRefreshing(true);
    try {
      await refreshWorkspaceData();
    } catch (err) {
      setStatus(String(err), "error");
      setRefreshing(false);
    }
  }

  async function handleLogout() {
    await logout();
    clearChannels();
    setLoggedOut();
  }

  return (
    <div className="top-bar">
      <div className="top-bar__filters">
        {FILTERS.map((filter) => (
          <button
            key={filter.key}
            className={
              filter.key === activeFilter ? "filter-button filter-button--active" : "filter-button"
            }
            onClick={() => setActiveFilter(filter.key)}
          >
            {filter.label}
          </button>
        ))}
      </div>
      <div className="top-bar__actions">
        <button onClick={handleRefresh} disabled={isRefreshing}>
          {isRefreshing ? "更新中..." : "更新"}
        </button>
        <button onClick={handleLogout}>ログアウト</button>
      </div>
    </div>
  );
}
