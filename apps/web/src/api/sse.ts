// Incremental parser for a `text/event-stream` body. `EventSource` cannot send
// an Authorization header, so the stream is read with fetch() and fed here.

export interface SseMessage {
  event: string;
  data: string;
}

export class SseParser {
  private buffer = "";

  /** Feeds the next chunk of text; returns every message completed by it. */
  push(chunk: string): SseMessage[] {
    // A "\r\n" split across two chunks is still joined, because the whole buffer is normalized.
    this.buffer = (this.buffer + chunk).replace(/\r\n/g, "\n");

    const messages: SseMessage[] = [];
    let boundary = this.buffer.indexOf("\n\n");
    while (boundary !== -1) {
      const frame = this.buffer.slice(0, boundary);
      this.buffer = this.buffer.slice(boundary + 2);
      const message = parseFrame(frame);
      if (message) messages.push(message);
      boundary = this.buffer.indexOf("\n\n");
    }
    return messages;
  }
}

function parseFrame(frame: string): SseMessage | null {
  let event = "message";
  const data: string[] = [];
  for (const line of frame.split("\n")) {
    if (line === "" || line.startsWith(":")) continue; // blank or comment (keep-alive)
    const colon = line.indexOf(":");
    const field = colon === -1 ? line : line.slice(0, colon);
    let value = colon === -1 ? "" : line.slice(colon + 1);
    if (value.startsWith(" ")) value = value.slice(1);
    if (field === "event") event = value;
    else if (field === "data") data.push(value);
  }
  // A frame with no data line (a bare comment, say) is not a message.
  return data.length === 0 ? null : { event, data: data.join("\n") };
}
