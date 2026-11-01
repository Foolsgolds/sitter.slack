import { useEffect, useState, type FormEvent } from "react";
import { normalizeBaseUrl, setConnection, testConnection } from "../api/client";
import { loadServerSettings, saveServerSettings, writeLog } from "../api/commands";
import { useAuthStore } from "../store/authStore";

const DEFAULT_SERVER_URL = "http://127.0.0.1:8787";

/** The connection screen: where the SlackSitter server is, and the API key for it. */
export function AuthenticationView() {
  const [serverUrl, setServerUrl] = useState(DEFAULT_SERVER_URL);
  const [apiKey, setApiKey] = useState("");
  const status = useAuthStore((s) => s.status);
  const errorMessage = useAuthStore((s) => s.errorMessage);
  const setAuthenticating = useAuthStore((s) => s.setAuthenticating);
  const setAuthenticated = useAuthStore((s) => s.setAuthenticated);
  const setLoggedOut = useAuthStore((s) => s.setLoggedOut);

  const busy = status === "authenticating";

  // Start from the saved connection, so a server that was down at startup
  // can simply be retried.
  useEffect(() => {
    let cancelled = false;
    loadServerSettings()
      .then((saved) => {
        if (cancelled || !saved) return;
        setServerUrl(saved.serverUrl);
        setApiKey(saved.apiKey);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    const url = normalizeBaseUrl(serverUrl);
    if (!/^https?:\/\/.+/i.test(url)) {
      setLoggedOut("サーバー URL は http:// か https:// で始めてください");
      return;
    }
    if (!apiKey.trim()) {
      setLoggedOut("API キーを入力してください");
      return;
    }

    setAuthenticating();
    try {
      // Reachability (/api/health) and the key (/api/me) are checked before anything is saved.
      const me = await testConnection({ baseUrl: url, apiKey });
      await saveServerSettings(url, apiKey.trim());
      setConnection({ baseUrl: url, apiKey });
      void writeLog("info", "connected to the server");
      setAuthenticated(me, url);
    } catch (err) {
      setLoggedOut(String(err));
    }
  }

  return (
    <div className="auth-view">
      <div className="auth-card">
        <h1>サーバー接続</h1>
        <div className="auth-instructions">
          <p>
            SlackSitter サーバー (slacksitter-server) を先に起動し、その URL と API キーを入力してください。
            Slack のトークンはサーバー側の設定に書きます。このアプリには入力しません。
          </p>
          <p>設定のしかたは SERVER.md を参照してください。</p>
        </div>
        <form onSubmit={handleSubmit} className="auth-form">
          <label htmlFor="server-url-input">サーバー URL</label>
          <input
            id="server-url-input"
            type="text"
            placeholder={DEFAULT_SERVER_URL}
            value={serverUrl}
            onChange={(event) => setServerUrl(event.target.value)}
            disabled={busy}
          />
          <label htmlFor="api-key-input">API キー</label>
          <input
            id="api-key-input"
            type="password"
            placeholder="slacksitter.toml の server.api_key"
            value={apiKey}
            onChange={(event) => setApiKey(event.target.value)}
            disabled={busy}
          />
          <button type="submit" disabled={busy}>
            {busy ? "接続中..." : "接続"}
          </button>
        </form>
        {errorMessage && <p className="auth-status auth-status--error">{errorMessage}</p>}
      </div>
    </div>
  );
}
