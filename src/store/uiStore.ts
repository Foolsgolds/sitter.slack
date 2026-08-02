import { create } from "zustand";
import type { BoardFilter } from "../types/models";

interface UiState {
  activeFilter: BoardFilter;
  isRefreshing: boolean;
  autoRefreshEnabled: boolean;
  imagePreviewSrc: string | null;
  statusMessage: string | null;
  statusKind: "loading" | "warning" | "error" | null;
  setActiveFilter: (filter: BoardFilter) => void;
  setRefreshing: (value: boolean) => void;
  setAutoRefreshEnabled: (value: boolean) => void;
  setImagePreview: (src: string | null) => void;
  setStatus: (message: string | null, kind: "loading" | "warning" | "error" | null) => void;
}

export const useUiStore = create<UiState>((set) => ({
  activeFilter: "JoinedOnly",
  isRefreshing: false,
  autoRefreshEnabled: true,
  imagePreviewSrc: null,
  statusMessage: null,
  statusKind: null,
  setActiveFilter: (filter) => set({ activeFilter: filter }),
  setRefreshing: (value) => set({ isRefreshing: value }),
  setAutoRefreshEnabled: (value) => set({ autoRefreshEnabled: value }),
  setImagePreview: (src) => set({ imagePreviewSrc: src }),
  setStatus: (statusMessage, statusKind) => set({ statusMessage, statusKind }),
}));
