import { create } from "zustand";
import type { ChannelWithMessages } from "../types/models";

interface ChannelsState {
  channels: Map<string, ChannelWithMessages>;
  upsertMany: (channels: ChannelWithMessages[]) => void;
  upsertOne: (channel: ChannelWithMessages) => void;
  clear: () => void;
}

export const useChannelsStore = create<ChannelsState>((set) => ({
  channels: new Map(),
  upsertMany: (incoming) =>
    set((state) => {
      const next = new Map(state.channels);
      for (const channel of incoming) next.set(channel.channelId, channel);
      return { channels: next };
    }),
  upsertOne: (channel) =>
    set((state) => {
      const next = new Map(state.channels);
      next.set(channel.channelId, channel);
      return { channels: next };
    }),
  clear: () => set({ channels: new Map() }),
}));

function parseTs(ts?: string): number {
  if (!ts) return -Infinity;
  const value = Number(ts);
  return Number.isNaN(value) ? -Infinity : value;
}

/** joined-first, then most-recent-message-first, then name (case-insensitive). */
export function sortChannels(channels: ChannelWithMessages[]): ChannelWithMessages[] {
  return [...channels].sort((a, b) => {
    if (a.isMember !== b.isMember) return a.isMember ? -1 : 1;
    const tsDiff = parseTs(b.lastMessageTs) - parseTs(a.lastMessageTs);
    if (tsDiff !== 0) return tsDiff;
    return a.name.toLowerCase().localeCompare(b.name.toLowerCase());
  });
}
