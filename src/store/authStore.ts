import { create } from "zustand";
import type { MeResult } from "../types/models";

/** `loggedOut` here means "not connected to a server" (the connection screen is shown). */
export type AuthStatus = "checking" | "loggedOut" | "authenticating" | "authenticated";

/** Used until `/api/me` has told us the server's real prefix. */
export const DEFAULT_CHANNEL_PREFIX = "times";

interface AuthState {
  status: AuthStatus;
  serverUrl?: string;
  workspaceUrl?: string;
  userId?: string;
  avatarUri?: string;
  /** Boards show the channels whose name starts with this; set by the server (`/api/me`). */
  channelPrefix: string;
  errorMessage?: string;
  setChecking: () => void;
  setAuthenticating: () => void;
  setAuthenticated: (result: MeResult, serverUrl: string) => void;
  setLoggedOut: (errorMessage?: string) => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  status: "checking",
  channelPrefix: DEFAULT_CHANNEL_PREFIX,
  setChecking: () => set({ status: "checking" }),
  setAuthenticating: () => set({ status: "authenticating", errorMessage: undefined }),
  setAuthenticated: (result, serverUrl) =>
    set({
      status: "authenticated",
      serverUrl,
      workspaceUrl: result.workspaceUrl,
      userId: result.userId,
      avatarUri: result.currentUserAvatarUri,
      channelPrefix: result.channelPrefix || DEFAULT_CHANNEL_PREFIX,
      errorMessage: undefined,
    }),
  setLoggedOut: (errorMessage) => set({ status: "loggedOut", errorMessage }),
}));
