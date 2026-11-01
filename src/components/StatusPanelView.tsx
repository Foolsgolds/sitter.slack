import { useUiStore } from "../store/uiStore";
import { REQUIRED_OAUTH_SCOPES } from "../constants";

export function StatusPanelView() {
  const statusMessage = useUiStore((s) => s.statusMessage);
  const statusKind = useUiStore((s) => s.statusKind);
  const serverReachable = useUiStore((s) => s.serverReachable);

  // A lost server outranks whatever else the panel was showing; it clears
  // itself when the connection comes back.
  if (!serverReachable) {
    return (
      <div className="status-panel status-panel--error">
        <p>サーバーに接続できません。再接続しています...</p>
      </div>
    );
  }

  if (!statusMessage) return null;

  // The server passes Slack's error through in its `sync-error` message ("missing_scope", ...).
  const showScopes = statusKind === "error" && /missing_scope|scope|権限/i.test(statusMessage);

  return (
    <div className={`status-panel status-panel--${statusKind}`}>
      <p>{statusMessage}</p>
      {showScopes && (
        <div className="required-scopes">
          <p>サーバーの Slack トークンに必要な OAuth スコープ:</p>
          <ul>
            {REQUIRED_OAUTH_SCOPES.map((scope) => (
              <li key={scope}>{scope}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
