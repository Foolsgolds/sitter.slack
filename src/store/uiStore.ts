import { create } from "zustand";
import type { BoardFilter } from "../types/models";

export type PopupId = "user" | "message";

export interface ProfileTarget {
  userId: string;
  /** Avatar already known from the message, used as a fallback in the dialog. */
  avatarUri?: string;
}

interface UiState {
  activeFilter: BoardFilter;
  isRefreshing: boolean;
  autoRefreshEnabled: boolean;
  imagePreviewSrc: string | null;
  statusMessage: string | null;
  statusKind: "loading" | "warning" | "error" | null;
  /** Only one bubble popup is open at a time. */
  openPopup: PopupId | null;
  profileTarget: ProfileTarget | null;
  setActiveFilter: (filter: BoardFilter) => void;
  setRefreshing: (value: boolean) => void;
  setAutoRefreshEnabled: (value: boolean) => void;
  setImagePreview: (src: string | null) => void;
  setStatus: (message: string | null, kind: "loading" | "warning" | "error" | null) => void;
  setOpenPopup: (id: PopupId | null) => void;
  setProfileTarget: (target: ProfileTarget | null) => void;
}

export const useUiStore = create<UiState>((set) => ({
  activeFilter: "JoinedOnly",
  isRefreshing: false,
  autoRefreshEnabled: true,
  imagePreviewSrc: null,
  statusMessage: null,
  statusKind: null,
  openPopup: null,
  profileTarget: null,
  setActiveFilter: (filter) => set({ activeFilter: filter }),
  setRefreshing: (value) => set({ isRefreshing: value }),
  setAutoRefreshEnabled: (value) => set({ autoRefreshEnabled: value }),
  setImagePreview: (src) => set({ imagePreviewSrc: src }),
  setStatus: (statusMessage, statusKind) => set({ statusMessage, statusKind }),
  setOpenPopup: (openPopup) => set({ openPopup }),
  setProfileTarget: (profileTarget) => set({ profileTarget, openPopup: null }),
}));
