import { useEffect } from "react";
import { saveCustomBoards } from "../api/commands";
import { useBoardStore } from "./boardStore";
import { useUiStore } from "./uiStore";

/**
 * Writes custom-board.json whenever the boards, the active board or the active
 * filter change (after the initial load, so the file is never clobbered first).
 */
export function usePersistBoardState() {
  const hydrated = useBoardStore((s) => s.hydrated);
  const customBoards = useBoardStore((s) => s.customBoards);
  const activeCustomBoardIndex = useBoardStore((s) => s.activeCustomBoardIndex);
  const activeFilter = useUiStore((s) => s.activeFilter);

  useEffect(() => {
    if (!hydrated) return;
    saveCustomBoards({ activeFilter, activeCustomBoardIndex, customBoards }).catch((err) => {
      useUiStore.getState().setStatus(`カスタムボードを保存できませんでした: ${String(err)}`, "warning");
    });
  }, [hydrated, customBoards, activeCustomBoardIndex, activeFilter]);
}
