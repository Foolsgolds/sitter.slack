import { useEffect, useState } from "react";
import { getUserProfile } from "../api/commands";
import { useUiStore } from "../store/uiStore";
import type { SlackUserProfileInfo } from "../types/models";

export function UserProfileDialog() {
  const target = useUiStore((s) => s.profileTarget);
  const setProfileTarget = useUiStore((s) => s.setProfileTarget);
  const [profile, setProfile] = useState<SlackUserProfileInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  const userId = target?.userId;

  useEffect(() => {
    if (!userId) return;
    let cancelled = false;
    setProfile(null);
    setError(null);
    getUserProfile(userId)
      .then((info) => {
        if (!cancelled) setProfile(info);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [userId]);

  useEffect(() => {
    if (!target) return;
    function handleKey(event: KeyboardEvent) {
      if (event.key === "Escape") setProfileTarget(null);
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [target, setProfileTarget]);

  if (!target) return null;

  const avatar = profile?.imageUrl || target.avatarUri;
  const heading = profile?.displayName || profile?.realName || profile?.userName || target.userId;

  // Only show rows that have a value, like the legacy dialog.
  const rows: [string, string | undefined][] = [
    ["ユーザー名", profile?.userName],
    ["表示名", profile?.displayName],
    ["本名", profile?.realName],
    ["肩書き", profile?.title],
    ["ステータス", profile?.statusText],
  ];

  return (
    <div className="dialog-overlay" onClick={() => setProfileTarget(null)}>
      <div
        className="dialog profile-dialog"
        role="dialog"
        aria-label="ユーザー情報"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="profile-dialog__header">
          <div className="profile-dialog__avatar">{avatar && <img src={avatar} alt="" />}</div>
          <div>
            <div className="profile-dialog__name">{heading}</div>
            <div className="profile-dialog__id">{target.userId}</div>
          </div>
        </div>

        {error && <p className="popup-status popup-status--error">{error}</p>}
        {!profile && !error && <p className="popup-hint">読み込み中...</p>}

        <dl className="profile-dialog__rows">
          {rows
            .filter(([, value]) => value)
            .map(([label, value]) => (
              <div key={label} className="profile-dialog__row">
                <dt>{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
        </dl>

        <div className="popup-actions">
          <button type="button" onClick={() => setProfileTarget(null)}>
            閉じる
          </button>
        </div>
      </div>
    </div>
  );
}
