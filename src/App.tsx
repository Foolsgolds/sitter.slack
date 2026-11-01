import { useEffect } from "react";
import { getMe, setConnection } from "./api/client";
import { loadServerSettings } from "./api/commands";
import { hydrateBoards, syncCustomTargets } from "./actions/boards";
import { loadWorkspace, startServerSession } from "./actions/session";
import { useAuthStore } from "./store/authStore";
import { useBoardStore } from "./store/boardStore";
import { useChannelsStore } from "./store/channelsStore";
import { usePersistBoardState } from "./store/usePersistBoardState";
import { selectVisibleChannels } from "./store/visibleChannels";
import { useUiStore } from "./store/uiStore";
import { AuthenticationView } from "./components/AuthenticationView";
import { StartupSplash } from "./components/StartupSplash";
import { StatusPanelView } from "./components/StatusPanelView";
import { ChannelBoardView } from "./components/ChannelBoardView";
import { MainControllerView } from "./components/MainControllerView";
import { ImagePreviewOverlay } from "./components/ImagePreviewOverlay";
import { UserProfileDialog } from "./components/UserProfileDialog";
import "./styles/app.css";

function App() {
  const status = useAuthStore((s) => s.status);
  const channelPrefix = useAuthStore((s) => s.channelPrefix);
  const setChecking = useAuthStore((s) => s.setChecking);
  const setAuthenticated = useAuthStore((s) => s.setAuthenticated);
  const setLoggedOut = useAuthStore((s) => s.setLoggedOut);

  const channels = useChannelsStore((s) => s.channels);
  const clearChannels = useChannelsStore((s) => s.clear);

  const customBoards = useBoardStore((s) => s.customBoards);
  const activeCustomBoardIndex = useBoardStore((s) => s.activeCustomBoardIndex);
  const boardsHydrated = useBoardStore((s) => s.hydrated);

  const activeFilter = useUiStore((s) => s.activeFilter);

  // Startup: try the saved server connection before showing the connection screen.
  useEffect(() => {
    let cancelled = false;
    setChecking();
    (async () => {
      const saved = await loadServerSettings();
      if (!saved) {
        if (!cancelled) setLoggedOut();
        return;
      }
      setConnection({ baseUrl: saved.serverUrl, apiKey: saved.apiKey });
      const me = await getMe();
      if (!cancelled) setAuthenticated(me, saved.serverUrl);
    })().catch((err) => {
      // The saved settings stay; the connection screen shows why and lets the user retry.
      if (!cancelled) setLoggedOut(String(err));
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Restore custom boards and the saved filter before the first load, so the
  // load knows which extra channels the boards need.
  useEffect(() => {
    hydrateBoards();
  }, []);
  usePersistBoardState();

  // Right after connecting: load from the server, tell it which channels the
  // custom boards need, and listen for its events (30-second polling while the
  // event stream is down). The server syncs Slack on its own, so there is no refresh timer here.
  useEffect(() => {
    if (status !== "authenticated" || !boardsHydrated) return;
    clearChannels();
    void loadWorkspace({ showLoading: true });
    syncCustomTargets();
    return startServerSession();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status, boardsHydrated]);

  if (status === "checking") return <StartupSplash />;
  if (status !== "authenticated") return <AuthenticationView />;

  const visibleChannels = selectVisibleChannels(
    channels,
    activeFilter,
    customBoards,
    activeCustomBoardIndex,
    channelPrefix,
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
