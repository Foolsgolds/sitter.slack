import { useCallback, useEffect, useRef, useState } from "react";
import { getServerLogs } from "../api/client";
import { getLogs } from "../api/commands";

type CopyState = "idle" | "copied" | "failed";
type LogSource = "app" | "server";

/**
 * Recent lines of a log, with reload and copy-to-clipboard. "アプリ" is this
 * app's own log (written by the Tauri shell); "サーバー" is the server's (`/api/logs`).
 */
export function LogPopup() {
  const [source, setSource] = useState<LogSource>("app");
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copyState, setCopyState] = useState<CopyState>("idle");
  const viewRef = useRef<HTMLPreElement>(null);

  const load = useCallback(() => {
    setText(null);
    const read = source === "app" ? getLogs() : getServerLogs();
    read
      .then((logs) => {
        setText(logs);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  }, [source]);

  useEffect(load, [load]);

  // Newest entries are at the bottom; start there.
  useEffect(() => {
    const view = viewRef.current;
    if (view) view.scrollTop = view.scrollHeight;
  }, [text]);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(text ?? "");
      setCopyState("copied");
    } catch {
      setCopyState("failed");
    }
    window.setTimeout(() => setCopyState("idle"), 2000);
  }

  return (
    <div className="popup-form">
      <label>ログ</label>
      <div className="log-tabs">
        <button
          type="button"
          className={source === "app" ? "log-tab log-tab--selected" : "log-tab"}
          onClick={() => setSource("app")}
        >
          アプリ
        </button>
        <button
          type="button"
          className={source === "server" ? "log-tab log-tab--selected" : "log-tab"}
          onClick={() => setSource("server")}
        >
          サーバー
        </button>
      </div>
      {error ? (
        <p className="popup-status popup-status--error">ログを読み込めませんでした: {error}</p>
      ) : (
        <pre className="log-view" ref={viewRef}>
          {text === null ? "読み込み中..." : text || "ログはまだありません。"}
        </pre>
      )}
      <div className="popup-actions">
        <button type="button" onClick={load}>
          更新
        </button>
        <button type="button" className="primary" onClick={() => void handleCopy()} disabled={!text}>
          {copyState === "copied" ? "コピーしました" : copyState === "failed" ? "コピーできません" : "コピー"}
        </button>
      </div>
    </div>
  );
}
