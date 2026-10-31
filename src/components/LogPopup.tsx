import { useCallback, useEffect, useRef, useState } from "react";
import { getLogs } from "../api/commands";

type CopyState = "idle" | "copied" | "failed";

/** Recent lines of the app log, with reload and copy-to-clipboard. */
export function LogPopup() {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copyState, setCopyState] = useState<CopyState>("idle");
  const viewRef = useRef<HTMLPreElement>(null);

  const load = useCallback(() => {
    getLogs()
      .then((logs) => {
        setText(logs);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  }, []);

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
