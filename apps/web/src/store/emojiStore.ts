import { create } from "zustand";

interface EmojiState {
  emojiMap: Record<string, string>;
  setEmojiMap: (map: Record<string, string>) => void;
}

export const useEmojiStore = create<EmojiState>((set) => ({
  emojiMap: {},
  setEmojiMap: (map) => set({ emojiMap: map }),
}));
