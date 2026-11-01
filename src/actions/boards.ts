// Custom-board operations. Like session.ts these use getState() so they can be
// called from event handlers; persistence is handled by usePersistBoardState.

import { loadChannels, putSyncTargets } from "../api/client";
import { loadCustomBoards, writeLog } from "../api/commands";
import { DEFAULT_BOARD_NAME, MAX_CUSTOM_BOARDS } from "../constants";
import { useBoardStore } from "../store/boardStore";
import { useChannelsStore } from "../store/channelsStore";
import { useUiStore } from "../store/uiStore";
import type { BoardFilter, CustomBoardDefinition } from "../types/models";

function normalizeName(name: string): string {
  const trimmed = name.trim();
  return trimmed === "" ? DEFAULT_BOARD_NAME : trimmed;
}

function uniqueIds(ids: string[]): string[] {
  return Array.from(new Set(ids));
}

/** Every channel any custom board refers to. */
export function customBoardChannelIds(): string[] {
  return uniqueIds(useBoardStore.getState().customBoards.flatMap((board) => board.channelIds));
}

/**
 * Tells the server which channels the boards need synced. The server keeps them
 * as its `custom` targets and starts syncing the ones it has never seen.
 */
export function syncCustomTargets() {
  putSyncTargets(customBoardChannelIds()).catch((err) => {
    void writeLog("warn", `could not update the server's sync targets: ${String(err)}`);
  });
}

/** Reads custom-board.json once at startup and restores the saved filter. */
export async function hydrateBoards() {
  try {
    const saved = await loadCustomBoards();
    useBoardStore.getState().setBoards(saved.customBoards, saved.activeCustomBoardIndex);
    useUiStore.getState().setActiveFilter(saved.activeFilter as BoardFilter);
  } catch (err) {
    // Starting with no custom boards beats not starting.
    console.error("failed to load custom boards", err);
  } finally {
    useBoardStore.getState().setHydrated();
  }
}

/** Fetches channels the store doesn't have yet (e.g. a freshly picked non-prefix one). */
export async function loadMissingChannels(channelIds: string[]) {
  const loaded = useChannelsStore.getState().channels;
  const missing = channelIds.filter((id) => !loaded.has(id));
  if (missing.length === 0) return;

  // A full refresh owns the status panel while it runs.
  const showStatus = !useUiStore.getState().isRefreshing;
  if (showStatus) useUiStore.getState().setStatus("チャンネルを読み込んでいます...", "loading");
  try {
    useChannelsStore.getState().upsertMany(await loadChannels(missing));
    if (showStatus && useUiStore.getState().statusKind === "loading") {
      useUiStore.getState().setStatus(null, null);
    }
  } catch (err) {
    useUiStore.getState().setStatus(String(err), "error");
  }
}

/** Returns false when the 10-board limit is already reached. */
export function addBoard(name: string, channelIds: string[]): boolean {
  const { customBoards, setBoards } = useBoardStore.getState();
  if (customBoards.length >= MAX_CUSTOM_BOARDS) return false;

  const board: CustomBoardDefinition = { name: normalizeName(name), channelIds: uniqueIds(channelIds) };
  setBoards([...customBoards, board], customBoards.length);
  useUiStore.getState().setActiveFilter("CustomOnly");
  useUiStore.getState().setOpenPopup(null);
  void writeLog("info", `custom board added: ${board.name} (${board.channelIds.length} channels)`);
  syncCustomTargets();
  void loadMissingChannels(board.channelIds);
  return true;
}

export function updateBoard(index: number, name: string, channelIds: string[]) {
  const { customBoards, setBoards } = useBoardStore.getState();
  if (index < 0 || index >= customBoards.length) return;

  const board: CustomBoardDefinition = { name: normalizeName(name), channelIds: uniqueIds(channelIds) };
  setBoards(
    customBoards.map((existing, i) => (i === index ? board : existing)),
    index,
  );
  useUiStore.getState().setOpenPopup(null);
  void writeLog("info", `custom board updated: ${board.name} (${board.channelIds.length} channels)`);
  syncCustomTargets();
  void loadMissingChannels(board.channelIds);
}

export function deleteBoard(index: number) {
  const { customBoards, activeCustomBoardIndex, setBoards } = useBoardStore.getState();
  if (index < 0 || index >= customBoards.length) return;

  const remaining = customBoards.filter((_, i) => i !== index);
  let nextActive = activeCustomBoardIndex;
  if (index < activeCustomBoardIndex) nextActive -= 1;
  nextActive = remaining.length === 0 ? -1 : Math.min(Math.max(nextActive, 0), remaining.length - 1);

  setBoards(remaining, nextActive);
  void writeLog("info", `custom board deleted: ${customBoards[index].name}`);
  syncCustomTargets();
  const ui = useUiStore.getState();
  if (ui.activeFilter === "CustomOnly") ui.setActiveFilter("JoinedOnly");
  ui.setOpenPopup(null);
}

/** Pressing the active board again goes back to "All", like ①/②. */
export function toggleCustomBoard(index: number) {
  const { customBoards, activeCustomBoardIndex, setBoards } = useBoardStore.getState();
  if (index < 0 || index >= customBoards.length) return;

  const ui = useUiStore.getState();
  if (ui.activeFilter === "CustomOnly" && activeCustomBoardIndex === index) {
    ui.setActiveFilter("All");
    return;
  }
  setBoards(customBoards, index);
  ui.setActiveFilter("CustomOnly");
  // Covers channels that were added while their board wasn't on screen.
  void loadMissingChannels(customBoards[index].channelIds);
}
