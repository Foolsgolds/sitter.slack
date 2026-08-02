import { create } from "zustand";
import type { AuthResult } from "../types/models";

export type AuthStatus = "checking" | "loggedOut" | "authenticating" | "authenticated";

interface AuthState {
  status: AuthStatus;
  workspaceUrl?: string;
  userId?: string;
  avatarUri?: string;
  errorMessage?: string;
  setChecking: () => void;
  setAuthenticating: () => void;
  setAuthenticated: (result: AuthResult) => void;
  setLoggedOut: (errorMessage?: string) => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  status: "checking",
  setChecking: () => set({ status: "checking" }),
  setAuthenticating: () => set({ status: "authenticating", errorMessage: undefined }),
  setAuthenticated: (result) =>
    set({
      status: "authenticated",
      workspaceUrl: result.workspaceUrl,
      userId: result.userId,
      avatarUri: result.currentUserAvatarUri,
      errorMessage: undefined,
    }),
  setLoggedOut: (errorMessage) => set({ status: "loggedOut", errorMessage }),
}));
