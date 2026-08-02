import type { MessageReactionItem } from "../types/models";
import { useEmojiStore } from "../store/emojiStore";
import { refreshChannel, toggleReaction } from "../api/commands";
import { useChannelsStore } from "../store/channelsStore";

interface Props {
  channelId: string;
  ts?: string;
  reactions: MessageReactionItem[];
}

export function ReactionBar({ channelId, ts, reactions }: Props) {
  const emojiMap = useEmojiStore((s) => s.emojiMap);
  const upsertOne = useChannelsStore((s) => s.upsertOne);

  if (reactions.length === 0) return null;

  async function handleClick(name: string) {
    if (!ts) return;
    try {
      await toggleReaction(channelId, ts, name);
      const updated = await refreshChannel(channelId);
      upsertOne(updated);
    } catch (err) {
      console.error("reaction toggle failed", err);
    }
  }

  return (
    <div className="reaction-bar">
      {reactions.map((reaction) => {
        const url = emojiMap[reaction.name];
        return (
          <button key={reaction.name} className="reaction-pill" onClick={() => handleClick(reaction.name)}>
            {url ? <img src={url} alt={reaction.name} /> : <span>{`:${reaction.name}:`}</span>}
            <span className="reaction-count">{reaction.count}</span>
          </button>
        );
      })}
    </div>
  );
}
