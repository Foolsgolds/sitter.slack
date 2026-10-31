// Thin typed wrappers over Tauri's invoke() — one function per command in
// src-tauri/src/commands/. Rust snake_case parameter names are matched to
// camelCase keys automatically by Tauri.

import { invoke } from "@tauri-apps/api/core";
import type {
  AuthResult,
  ChannelSummary,
  ChannelWithMessages,
  CustomBoardState,
  SlackUserProfileInfo,
} from "../types/models";

export const authenticate = (tokenValue: string) =>
  invoke<AuthResult>("authenticate", { tokenValue });

export const tryAutoLogin = () => invoke<AuthResult | null>("try_auto_login");

export const logout = () => invoke<void>("logout");

/** `extraChannelIds`: non-"times*" channels that custom boards need loaded too. */
export const refreshWorkspaceData = (extraChannelIds: string[] = []) =>
  invoke<void>("refresh_workspace_data", { extraChannelIds });

export const refreshChannel = (channelId: string) =>
  invoke<ChannelWithMessages>("refresh_channel", { channelId });

export const loadChannels = (channelIds: string[]) =>
  invoke<ChannelWithMessages[]>("load_channels", { channelIds });

export const listAllChannels = () => invoke<ChannelSummary[]>("list_all_channels");

export const getUserProfile = (userId: string) =>
  invoke<SlackUserProfileInfo>("get_user_profile", { userId });

export const getEmojiMap = () => invoke<Record<string, string>>("get_emoji_map");

export const getLogs = () => invoke<string>("get_logs");

export const postMessage = (channel: string, text: string) =>
  invoke<void>("post_message", { channel, text });

export const toggleReaction = (channelId: string, ts: string, name: string) =>
  invoke<void>("toggle_reaction", { channelId, ts, name });

export const fetchImage = (candidateUrls: string[]) =>
  invoke<string>("fetch_image", { candidateUrls });

export const loadCustomBoards = () => invoke<CustomBoardState>("load_custom_boards");

export const saveCustomBoards = (boardState: CustomBoardState) =>
  invoke<void>("save_custom_boards", { boardState });

/** Records a UI-side event in the shared log file. Never pass secrets. */
export const writeLog = (level: "info" | "warn" | "error", message: string) =>
  invoke<void>("write_log", { level, message }).catch(() => {});
