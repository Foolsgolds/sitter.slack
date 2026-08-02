// Mirrors the Rust display models in src-tauri/src/slack/models.rs
// (serde `rename_all = "camelCase"`). Keep in sync by hand for now.

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
