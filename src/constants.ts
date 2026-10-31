export const REQUIRED_OAUTH_SCOPES = [
  "channels:read",
  "channels:history",
  "groups:read",
  "groups:history",
  "mpim:read",
  "mpim:history",
  "im:read",
  "im:history",
  "users:read",
  "chat:write",
  "emoji:read",
];

/**
 * Only channels whose name starts with this (case-sensitive) prefix appear on
 * the ALL / joined / not-joined boards. Must match CHANNEL_NAME_PREFIX in
 * src-tauri/src/commands/channels.rs. Custom boards may hold any channel.
 */
export const CHANNEL_NAME_PREFIX = "times";

export const MAX_CUSTOM_BOARDS = 10;
export const DEFAULT_BOARD_NAME = "カスタムボード";
