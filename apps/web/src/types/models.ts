// Mirrors the Rust display models in server/core/src/slack/models.rs
// (serde `rename_all = "camelCase"`), which the server returns as JSON.
// Keep in sync by hand.

export type SegmentType = "text" | "link" | "emoji";

export interface MessageInlineSegment {
  segType: SegmentType;
  text: string;
  uri?: string;
  isBold?: boolean;
  isItalic?: boolean;
  isStrikethrough?: boolean;
  isCode?: boolean;
}

export interface MessageImageItem {
  candidateUrls: string[];
}

export interface MessageReactionItem {
  name: string;
  count: number;
}

export interface MessageDisplayItem {
  user?: string;
  ts?: string;
  text?: string;
  userAvatarUri?: string;
  permalinkUri?: string;
  segments: MessageInlineSegment[];
  images: MessageImageItem[];
  reactions: MessageReactionItem[];
  replies: MessageDisplayItem[];
}

export interface ChannelWithMessages {
  channelId: string;
  channelUri?: string;
  name: string;
  topicValue?: string;
  purposeValue?: string;
  numMembers?: number;
  isMember: boolean;
  lastMessageTs?: string;
  lastFetchedTs?: string;
  messages: MessageDisplayItem[];
}

export interface ChannelSummary {
  channelId: string;
  name: string;
  isMember: boolean;
  isArchived: boolean;
}

export interface SlackUserProfileInfo {
  userId: string;
  userName: string;
  displayName: string;
  realName: string;
  title: string;
  statusText: string;
  imageUrl: string;
}

export interface AuthResult {
  workspaceUrl: string;
  userId: string;
  currentUserAvatarUri?: string;
}

/** `GET /api/me`: the signed-in user plus the board channel prefix the server syncs. */
export interface MeResult extends AuthResult {
  channelPrefix: string;
}

/** Server URL and API key saved by the Tauri shell. */
export interface ServerSettings {
  serverUrl: string;
  apiKey: string;
}

export interface ChannelSyncStatus {
  channelId: string;
  /** `prefix`, `custom`, or null when the channel is no longer a target. */
  source: "prefix" | "custom" | null;
  lastMessageTs: string | null;
  /** Unix seconds. */
  lastSyncedAt: number | null;
  lastError: string | null;
}

/** `GET /api/sync/status` (times are Unix seconds). */
export interface SyncStatus {
  running: boolean;
  lastStartedAt: number | null;
  lastFinishedAt: number | null;
  lastCatalogAt: number | null;
  channelPrefix: string;
  channels: ChannelSyncStatus[];
}

export type BoardFilter = "All" | "JoinedOnly" | "NotJoinedOnly" | "CustomOnly";

export interface CustomBoardDefinition {
  name: string;
  channelIds: string[];
}

export interface CustomBoardState {
  activeFilter: string;
  activeCustomBoardIndex: number;
  customBoards: CustomBoardDefinition[];
}
