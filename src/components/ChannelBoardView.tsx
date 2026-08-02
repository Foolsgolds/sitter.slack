import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { ChannelWithMessages } from "../types/models";
import { ChannelCardView, cardWidthFor } from "./ChannelCardView";

const CARD_GAP = 16;

/** Horizontal windowed rendering of channel columns, replacing the legacy
 * hand-rolled `HorizontalVirtualizingLayout`. */
export function ChannelBoardView({ channels }: { channels: ChannelWithMessages[] }) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: channels.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => cardWidthFor(channels[index]) + CARD_GAP,
    horizontal: true,
    overscan: 2,
  });

  if (channels.length === 0) {
    return <div className="channel-board channel-board--empty" ref={parentRef} />;
  }

  return (
    <div className="channel-board" ref={parentRef}>
      <div style={{ width: virtualizer.getTotalSize(), position: "relative", height: "100%" }}>
        {virtualizer.getVirtualItems().map((virtualItem) => {
          const channel = channels[virtualItem.index];
          return (
            <div
              key={channel.channelId}
              ref={virtualizer.measureElement}
              data-index={virtualItem.index}
              className="channel-board__slot"
              style={{
                position: "absolute",
                top: 0,
                left: virtualItem.start,
                height: "100%",
              }}
            >
              <ChannelCardView channel={channel} />
            </div>
          );
        })}
      </div>
    </div>
  );
}
