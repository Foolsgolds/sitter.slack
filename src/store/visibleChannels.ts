import type { BoardFilter, ChannelWithMessages, CustomBoardDefinition } from "../types/models";
import { sortChannels } from "./channelsStore";

/**
 * Channels shown for the active filter. The standard filters only cover the
 * channels named with `channelPrefix` (the server's `channel_prefix`, from
 * `/api/me`); the store may also hold other channels loaded for custom boards.
 * A custom board shows exactly its channels, in the order chosen.
 */
export function selectVisibleChannels(
  channels: Map<string, ChannelWithMessages>,
  filter: BoardFilter,
  customBoards: CustomBoardDefinition[],
  activeCustomBoardIndex: number,
  channelPrefix: string,
): ChannelWithMessages[] {
  if (filter === "CustomOnly") {
    const board = customBoards[activeCustomBoardIndex];
    if (!board) return [];
    return board.channelIds.flatMap((id) => {
      const channel = channels.get(id);
      return channel ? [channel] : [];
    });
  }

  const boardChannels = Array.from(channels.values()).filter((channel) =>
    channel.name.startsWith(channelPrefix),
  );
  switch (filter) {
    case "JoinedOnly":
      return sortChannels(boardChannels.filter((channel) => channel.isMember));
    case "NotJoinedOnly":
      return sortChannels(boardChannels.filter((channel) => !channel.isMember));
    default:
      return sortChannels(boardChannels);
  }
}
