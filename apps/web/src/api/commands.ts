// Thin typed wrappers over Tauri's invoke() — one function per command in
// apps/desktop/src-tauri/src/commands/. The Tauri shell only keeps local state now (server
// connection, custom boards, the UI log); everything about Slack goes through
// the server (see client.ts). Rust snake_case parameter names are matched to
// camelCase keys automatically by Tauri.

import { invoke } from "@tauri-apps/api/core";
import type { CustomBoardState, ServerSettings } from "../types/models";

/** The saved server URL + API key, or null when none is saved yet. */
export const loadServerSettings = () => invoke<ServerSettings | null>("load_server_settings");

export const saveServerSettings = (serverUrl: string, apiKey: string) =>
  invoke<void>("save_server_settings", { serverUrl, apiKey });

export const clearServerSettings = () => invoke<void>("clear_server_settings");

/** The Tauri shell's own log (UI events). The server's log is `getServerLogs`. */
export const getLogs = () => invoke<string>("get_logs");

export const loadCustomBoards = () => invoke<CustomBoardState>("load_custom_boards");

export const saveCustomBoards = (boardState: CustomBoardState) =>
  invoke<void>("save_custom_boards", { boardState });

/** Records a UI-side event in the shared log file. Never pass secrets. */
export const writeLog = (level: "info" | "warn" | "error", message: string) =>
  invoke<void>("write_log", { level, message }).catch(() => {});
