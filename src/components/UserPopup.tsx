import { useEffect, useState } from "react";
import { ApiError, getSyncStatus, getUserProfile, syncAll } from "../api/client";
import { performLogout } from "../actions/session";
import { useAuthStore } from "../store/authStore";
import type { SlackUserProfileInfo, SyncStatus } from "../types/models";

type SyncNow = { kind: "busy" | "ok" | "error"; text: string } | null;

function formatUnix(seconds: number | null): string {
  if (seconds === null) return "まだありません";
  const date = new Date(seconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getMonth() + 1}/${date.getDate()} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function UserPopup() {
  const userId = useAuthStore((s) => s.userId);
  const avatarUri = useAuthStore((s) => s.avatarUri);
  const serverUrl = useAuthStore((s) => s.serverUrl);

  const [profile, setProfile] = useState<SlackUserProfileInfo | null>(null);
  const [syncStatus, setSyncStatus] = useState<SyncStatus | null>(null);
  const [syncNow, setSyncNow] = useState<SyncNow>(null);

  useEffect(() => {
    if (!userId) return;
    let cancelled = false;
    getUserProfile(userId)
      .then((info) => {
        if (!cancelled) setProfile(info);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [userId]);

  useEffect(() => {
    let cancelled = false;
    getSyncStatus()
      .then((status) => {
        if (!cancelled) setSyncStatus(status);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  async function handleSyncNow() {
    setSyncNow({ kind: "busy", text: "同期を依頼しています..." });
    try {
      await syncAll();
      setSyncNow({ kind: "ok", text: "同期を始めました。更新されたチャンネルから順に表示されます。" });
    } catch (err) {
      const running = err instanceof ApiError && err.status === 409;
      setSyncNow({
        kind: running ? "ok" : "error",
        text: running ? "同期はすでに実行中です。" : `同期を依頼できませんでした: ${String(err)}`,
      });
    }
  }

  const displayName = profile?.displayName || profile?.realName || profile?.userName || "";
  const busy = syncNow?.kind === "busy";

  return (
    <div className="user-popup">
      <div className="user-popup__header">
        <div className="user-popup__avatar">
          {(profile?.imageUrl || avatarUri) && <img src={profile?.imageUrl || avatarUri} alt="" />}
        </div>
        <div>
          <div className="user-popup__name">{displayName}</div>
          <div className="user-popup__id">{userId}</div>
        </div>
      </div>

      <hr />

      <div className="popup-form">
        <label>サーバー</label>
        <p className="popup-hint">{serverUrl}</p>
        <p className="popup-hint">
          サーバーが Slack を定期的に巡回して更新します。最後に同期した時刻:{" "}
          {formatUnix(syncStatus?.lastFinishedAt ?? null)}
          {syncStatus?.running ? " (同期中)" : ""}
        </p>
        <button type="button" onClick={() => void handleSyncNow()} disabled={busy}>
          今すぐ全体を同期
        </button>
        {syncNow && (
          <p
            className={
              syncNow.kind === "error"
                ? "popup-status popup-status--error"
                : syncNow.kind === "ok"
                  ? "popup-status popup-status--ok"
                  : "popup-status"
            }
          >
            {syncNow.text}
          </p>
        )}
      </div>

      <hr />

      <button type="button" className="danger" onClick={() => void performLogout()}>
        接続を解除
      </button>
      <p className="popup-hint">保存したサーバー URL と API キーを削除し、接続画面に戻ります。</p>
    </div>
  );
}
