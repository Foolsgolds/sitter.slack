import { create } from "zustand";
import type { CustomBoardDefinition } from "../types/models";

interface BoardState {
  customBoards: CustomBoardDefinition[];
  /** Last selected custom board; -1 when there are none. */
  activeCustomBoardIndex: number;
  /** False until custom-board.json has been read, so nothing is saved over it early. */
  hydrated: boolean;
  setBoards: (boards: CustomBoardDefinition[], activeIndex: number) => void;
  setHydrated: () => void;
}

export const useBoardStore = create<BoardState>((set) => ({
  customBoards: [],
  activeCustomBoardIndex: -1,
  hydrated: false,
  setBoards: (customBoards, activeCustomBoardIndex) => set({ customBoards, activeCustomBoardIndex }),
  setHydrated: () => set({ hydrated: true }),
}));
