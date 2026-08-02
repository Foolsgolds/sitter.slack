import type { MessageDisplayItem } from "../types/models";
import { MessageBody } from "./MessageBody";
import { MessageImages } from "./MessageImages";
import { ReactionBar } from "./ReactionBar";

function formatTimestamp(ts?: string): string {
  if (!ts) return "";
  const seconds = Number(ts);
  if (Number.isNaN(seconds)) return "";
  const date = new Date(seconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

interface Props {
  channelId: string;
  message: MessageDisplayItem;
  depth?: number;
}

export function MessageItemView({ channelId, message, depth = 0 }: Props) {
  return (
    <div className="message-item" style={depth > 0 ? { marginLeft: 28 } : undefined}>
      <div className="message-item__row">
        <div className="message-avatar">
          {message.userAvatarUri && <img src={message.userAvatarUri} alt="" />}
        </div>
        <div className="message-card">
          <a className="message-timestamp" href={message.permalinkUri} target="_blank" rel="noreferrer">
            {formatTimestamp(message.ts)}
          </a>
          <MessageBody segments={message.segments} />
          <MessageImages images={message.images} />
          <ReactionBar channelId={channelId} ts={message.ts} reactions={message.reactions} />
        </div>
      </div>
      {message.replies.length > 0 && (
        <div className="message-replies">
          {message.replies.map((reply, index) => (
            <MessageItemView key={reply.ts ?? index} channelId={channelId} message={reply} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  );
}
