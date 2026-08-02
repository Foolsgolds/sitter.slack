import { listen } from "@tauri-apps/api/event";
import type { ChannelWithMessages } from "../types/models";

/** Fires once per batch of "times*" channels as `refresh_workspace_data` streams them in. */
export function onChannelBatch(handler: (channels: ChannelWithMessages[]) => void) {
  return listen<ChannelWithMessages[]>("channel-batch", (event) => handler(event.payload));
}

export function onRefreshComplete(handler: () => void) {
  return listen("refresh-complete", () => handler());
}

export function onRefreshError(handler: (message: string) => void) {
  return listen<string>("refresh-error", (event) => handler(event.payload));
}
