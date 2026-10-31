import { useEffect } from "react";
import { getEmojiMap, tryAutoLogin } from "./api/commands";
import { onChannelBatch, onRefreshComplete, onRefreshError } from "./api/events";
import { startWorkspaceRefresh } from "./actions/session";
import { useAuthStore } from "./store/authStore";
import { sortChannels, useChannelsStore } from "./store/channelsStore";
import { useUiStore } from "./store/uiStore";
import { useEmojiStore } from "./store/emojiStore";
import { AuthenticationView } from "./components/AuthenticationView";
import { StartupSplash } from "./components/StartupSplash";
import { StatusPanelView } from "./components/StatusPanelView";
import { ChannelBoardView } from "./components/ChannelBoardView";
import { MainControllerView } from "./components/MainControllerView";
import { ImagePreviewOverlay } from "./components/ImagePreviewOverlay";
import { UserProfileDialog } from "./components/UserProfileDialog";
import "./styles/app.css";

const AUTO_REFRESH_INTERVAL_MS = 5 * 60 * 1000;

function App() {
  const status = useAuthStore((s) => s.status);
  const setChecking = useAuthStore((s) => s.setChecking);
  const setAuthenticated = useAuthStore((s) => s.setAuthenticated);
  const setLoggedOut = useAuthStore((s) => s.setLoggedOut);

  const channels = useChannelsStore((s) => s.channels);
  const upsertMany = useChannelsStore((s) => s.upsertMany);
  const clearChannels = useChannelsStore((s) => s.clear);

  const activeFilter = useUiStore((s) => s.activeFilter);
  const autoRefreshEnabled = useUiStore((s) => s.autoRefreshEnabled);
  const setRefreshing = useUiStore((s) => s.setRefreshing);
  const setStatus = useUiStore((s) => s.setStatus);

  const setEmojiMap = useEmojiStore((s) => s.setEmojiMap);

  // Startup: try the saved token before showing the login screen.
  useEffect(() => {
    let cancelled = false;
    setChecking();
    tryAutoLogin()
      .then((result) => {
        if (cancelled) return;
        if (result) setAuthenticated(result);
        else setLoggedOut();
      })
      .catch(() => {
        if (!cancelled) setLoggedOut();
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Subscribe once to the refresh_workspace_data streaming events.
  useEffect(() => {
    const unlistenBatch = onChannelBatch((batch) => upsertMany(batch));
    const unlistenComplete = onRefreshComplete(() => {
      setRefreshing(false);
      setStatus(null, null);
      getEmojiMap().then(setEmojiMap).catch(() => {});
    });
    const unlistenError = onRefreshError((message) => {
      setRefreshing(false);
      setStatus(message, "error");
    });
    return () => {
      unlistenBatch.then((unlisten) => unlisten());
      unlistenComplete.then((unlisten) => unlisten());
      unlistenError.then((unlisten) => unlisten());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Full workspace load right after authentication.
  useEffect(() => {
    if (status !== "authenticated") return;
    clearChannels();
    startWorkspaceRefresh({ showLoading: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status]);

  // 5-minute auto-refresh; startWorkspaceRefresh skips while one is in flight.
  useEffect(() => {
    if (status !== "authenticated" || !autoRefreshEnabled) return;
    const id = window.setInterval(() => startWorkspaceRefresh(), AUTO_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [status, autoRefreshEnabled]);

  if (status === "checking") return <StartupSplash />;
  if (status !== "authenticated") return <AuthenticationView />;

  const visibleChannels = sortChannels(
    Array.from(channels.values()).filter((channel) => {
      if (activeFilter === "All") return true;
      if (activeFilter === "JoinedOnly") return channel.isMember;
      if (activeFilter === "NotJoinedOnly") return !channel.isMember;
      return false; // CustomOnly lands in M3
    }),
  );

  return (
    <div className="app-shell">
      <StatusPanelView />
      <ChannelBoardView channels={visibleChannels} />
      <MainControllerView />
      <ImagePreviewOverlay />
      <UserProfileDialog />
    </div>
  );
}

export default App;
