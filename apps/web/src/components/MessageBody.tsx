import type { MessageInlineSegment } from "../types/models";
import { useEmojiStore } from "../store/emojiStore";

export function MessageBody({ segments }: { segments: MessageInlineSegment[] }) {
  const emojiMap = useEmojiStore((s) => s.emojiMap);

  return (
    <p className="message-body">
      {segments.map((segment, index) => {
        if (segment.segType === "link") {
          return (
            <a key={index} href={segment.uri} target="_blank" rel="noreferrer">
              {segment.text}
            </a>
          );
        }

        if (segment.segType === "emoji") {
          const url = emojiMap[segment.text];
          return url ? (
            <img key={index} className="inline-emoji" src={url} alt={`:${segment.text}:`} />
          ) : (
            <span key={index}>{`:${segment.text}:`}</span>
          );
        }

        const classNames = [
          segment.isBold && "text-bold",
          segment.isItalic && "text-italic",
          segment.isStrikethrough && "text-strike",
          segment.isCode && "text-code",
        ]
          .filter(Boolean)
          .join(" ");

        return (
          <span key={index} className={classNames || undefined}>
            {segment.text}
          </span>
        );
      })}
    </p>
  );
}
