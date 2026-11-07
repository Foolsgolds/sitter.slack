import type { ChannelWithMessages } from "../types/models";
import { MessageItemView } from "./MessageItemView";

const DEFAULT_CARD_WIDTH = 320;
const THREADED_CARD_WIDTH = 356;

function hasThreadReplies(channel: ChannelWithMessages): boolean {
  return channel.messages.some((m) => m.replies.length > 0);
}

export function cardWidthFor(channel: ChannelWithMessages): number {
  return hasThreadReplies(channel) ? THREADED_CARD_WIDTH : DEFAULT_CARD_WIDTH;
}

export function ChannelCardView({ channel }: { channel: ChannelWithMessages }) {
  const width = cardWidthFor(channel);
  const headerClass = channel.isMember
    ? "channel-card__header channel-card__header--joined"
    : "channel-card__header channel-card__header--not-joined";

  return (
    <div className="channel-card" style={{ width }}>
      <div className={headerClass}>
        <a href={channel.channelUri} target="_blank" rel="noreferrer">
          {channel.name}
        </a>
      </div>
      <div className="channel-card__messages">
        {channel.messages.length === 0 && (
          // A channel the server has not synced yet looks like an empty one.
          <p className="popup-hint">メッセージがありません (サーバーの同期待ちの可能性があります)</p>
        )}
        {channel.messages.map((message, index) => (
          <MessageItemView key={message.ts ?? index} channelId={channel.channelId} message={message} />
        ))}
      </div>
    </div>
  );
}
