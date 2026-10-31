import { useEffect, useState, type FormEvent } from "react";
import { authenticate, getUserProfile } from "../api/commands";
import { performLogout, startWorkspaceRefresh } from "../actions/session";
import { useAuthStore } from "../store/authStore";
import { useUiStore } from "../store/uiStore";
import type { SlackUserProfileInfo } from "../types/models";

type TokenStatus = { kind: "busy" | "ok" | "error"; text: string } | null;

/** How long the success message stays visible before the popup closes. */
const TOKEN_UPDATED_CLOSE_DELAY_MS = 2000;

export function UserPopup() {
  const userId = useAuthStore((s) => s.userId);
  const avatarUri = useAuthStore((s) => s.avatarUri);
  const setAuthenticated = useAuthStore((s) => s.setAuthenticated);
  const autoRefreshEnabled = useUiStore((s) => s.autoRefreshEnabled);
  const setAutoRefreshEnabled = useUiStore((s) => s.setAutoRefreshEnabled);
  const setOpenPopup = useUiStore((s) => s.setOpenPopup);

  const [profile, setProfile] = useState<SlackUserProfileInfo | null>(null);
  const [token, setToken] = useState("");
  const [tokenStatus, setTokenStatus] = useState<TokenStatus>(null);

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

  async function handleUpdateToken(event: FormEvent) {
    event.preventDefault();
    if (!token.trim()) {
      setTokenStatus({ kind: "error", text: "トークンを入力してください" });
      return;
    }

    setTokenStatus({ kind: "busy", text: "認証中..." });
    try {
      const result = await authenticate(token.trim());
      setAuthenticated(result);
      setToken("");
      setTokenStatus({ kind: "ok", text: "✅ トークンを更新しました" });
      startWorkspaceRefresh({ showLoading: true });
      window.setTimeout(() => setOpenPopup(null), TOKEN_UPDATED_CLOSE_DELAY_MS);
    } catch (err) {
      setTokenStatus({ kind: "error", text: `❌ 認証に失敗しました: ${String(err)}` });
    }
  }

  const displayName = profile?.displayName || profile?.realName || profile?.userName || "";
  const busy = tokenStatus?.kind === "busy";

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

      <form className="popup-form" onSubmit={handleUpdateToken}>
        <label htmlFor="new-token">トークンの変更</label>
        <input
          id="new-token"
          type="password"
          value={token}
          onChange={(event) => setToken(event.target.value)}
          placeholder="新しいトークン (xoxp-...)"
          disabled={busy}
        />
        <button type="submit" className="primary" disabled={busy}>
          トークンを更新
        </button>
        {tokenStatus && (
          <p
            className={
              tokenStatus.kind === "error"
                ? "popup-status popup-status--error"
                : tokenStatus.kind === "ok"
                  ? "popup-status popup-status--ok"
                  : "popup-status"
            }
          >
            {tokenStatus.text}
          </p>
        )}
        <p className="popup-hint">トークンはこのPCの資格情報ストアに保存されます。</p>
      </form>

      <hr />

      <div className="popup-form">
        <label>自動更新</label>
        <p className="popup-hint">
          {autoRefreshEnabled ? "5分ごとに自動で再取得しています。" : "自動更新は停止中です。"}
        </p>
        <button type="button" onClick={() => setAutoRefreshEnabled(!autoRefreshEnabled)}>
          {autoRefreshEnabled ? "自動更新を停止" : "自動更新を再開"}
        </button>
      </div>

      <hr />

      <button type="button" className="danger" onClick={() => void performLogout()}>
        ログアウト
      </button>
    </div>
  );
}
