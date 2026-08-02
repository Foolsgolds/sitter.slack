import { useState, type FormEvent } from "react";
import { authenticate } from "../api/commands";
import { useAuthStore } from "../store/authStore";
import { REQUIRED_OAUTH_SCOPES } from "../constants";

export function AuthenticationView() {
  const [token, setToken] = useState("");
  const status = useAuthStore((s) => s.status);
  const errorMessage = useAuthStore((s) => s.errorMessage);
  const setAuthenticating = useAuthStore((s) => s.setAuthenticating);
  const setAuthenticated = useAuthStore((s) => s.setAuthenticated);
  const setLoggedOut = useAuthStore((s) => s.setLoggedOut);

  const busy = status === "authenticating";

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!token.trim()) {
      setLoggedOut("User OAuth Token を入力してください");
      return;
    }
    setAuthenticating();
    try {
      const result = await authenticate(token.trim());
      setAuthenticated(result);
    } catch (err) {
      setLoggedOut(String(err));
    }
  }

  return (
    <div className="auth-view">
      <div className="auth-card">
        <h1>Slack 認証</h1>
        <div className="auth-instructions">
          <p>1. Slack の「Your Apps」からアプリを作成し、User Token Scopes に以下を追加してください。</p>
          <code className="scope-list">{REQUIRED_OAUTH_SCOPES.join(", ")}</code>
          <p>2. ワークスペースにインストールし、発行された User OAuth Token (xoxp-...) を下に貼り付けてください。</p>
        </div>
        <form onSubmit={handleSubmit} className="auth-form">
          <label htmlFor="token-input">User OAuth Token</label>
          <input
            id="token-input"
            type="password"
            placeholder="xoxp-..."
            value={token}
            onChange={(event) => setToken(event.target.value)}
            disabled={busy}
          />
          <button type="submit" disabled={busy}>
            {busy ? "認証中..." : "認証"}
          </button>
        </form>
        {errorMessage && <p className="auth-status auth-status--error">{errorMessage}</p>}
      </div>
    </div>
  );
}
