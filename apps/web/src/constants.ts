// Scopes the server's Slack token needs (shown when the server reports `missing_scope`).
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

export const MAX_CUSTOM_BOARDS = 10;
export const DEFAULT_BOARD_NAME = "カスタムボード";
