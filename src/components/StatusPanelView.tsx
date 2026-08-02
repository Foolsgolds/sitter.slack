import { useUiStore } from "../store/uiStore";
import { REQUIRED_OAUTH_SCOPES } from "../constants";

export function StatusPanelView() {
  const statusMessage = useUiStore((s) => s.statusMessage);
  const statusKind = useUiStore((s) => s.statusKind);

  if (!statusMessage) return null;

  const showScopes = statusKind === "error" && /scope|権限/i.test(statusMessage);

  return (
    <div className={`status-panel status-panel--${statusKind}`}>
      <p>{statusMessage}</p>
      {showScopes && (
        <div className="required-scopes">
          <p>必要な OAuth スコープ:</p>
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
