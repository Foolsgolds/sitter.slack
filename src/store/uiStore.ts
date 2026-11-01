import { create } from "zustand";
import { releaseObjectUrl, retainObjectUrl } from "../api/client";
import type { BoardFilter } from "../types/models";

export type PopupId = "log" | "user" | "message" | "boardSettings" | "boardAdd";

export interface ProfileTarget {
  userId: string;
  /** Avatar already known from the message, used as a fallback in the dialog. */
  avatarUri?: string;
}

interface UiState {
  activeFilter: BoardFilter;
  isRefreshing: boolean;
  /** False while the server cannot be reached (the status panel says so). */
  serverReachable: boolean;
  imagePreviewSrc: string | null;
  statusMessage: string | null;
  statusKind: "loading" | "warning" | "error" | null;
  /** Only one bubble popup is open at a time. */
  openPopup: PopupId | null;
  profileTarget: ProfileTarget | null;
  setActiveFilter: (filter: BoardFilter) => void;
  setRefreshing: (value: boolean) => void;
  setServerReachable: (value: boolean) => void;
  setImagePreview: (src: string | null) => void;
  setStatus: (message: string | null, kind: "loading" | "warning" | "error" | null) => void;
  setOpenPopup: (id: PopupId | null) => void;
  setProfileTarget: (target: ProfileTarget | null) => void;
}

export const useUiStore = create<UiState>((set, get) => ({
  activeFilter: "JoinedOnly",
  isRefreshing: false,
  serverReachable: true,
  imagePreviewSrc: null,
  statusMessage: null,
  statusKind: null,
  openPopup: null,
  profileTarget: null,
  setActiveFilter: (filter) => set({ activeFilter: filter }),
  setRefreshing: (value) => set({ isRefreshing: value }),
  setServerReachable: (serverReachable) => set({ serverReachable }),
  // The preview keeps its image's object URL alive while it is open.
  setImagePreview: (src) => {
    const previous = get().imagePreviewSrc;
    if (src) retainObjectUrl(src);
    if (previous) releaseObjectUrl(previous);
    set({ imagePreviewSrc: src });
  },
  setStatus: (statusMessage, statusKind) => set({ statusMessage, statusKind }),
  setOpenPopup: (openPopup) => set({ openPopup }),
  setProfileTarget: (profileTarget) => set({ profileTarget, openPopup: null }),
}));
