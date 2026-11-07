import { useAuthStore } from "../store/authStore";

export function StartupSplash() {
  const connectNote = useAuthStore((s) => s.connectNote);
  const setLoggedOut = useAuthStore((s) => s.setLoggedOut);

  return (
    <div className="startup-splash">
      <div className="startup-splash__icon">S</div>
      <h1>SlackSitter</h1>
      <div className="spinner" aria-label="loading" />
      {connectNote && (
        <div className="startup-splash__note">
          <p>サーバーに接続しています...</p>
          <p className="popup-hint">{connectNote}</p>
          {/* Leaves the retry loop for the connection screen (saved settings stay). */}
          <button type="button" onClick={() => setLoggedOut()}>
            接続設定を変える
          </button>
        </div>
      )}
    </div>
  );
}
