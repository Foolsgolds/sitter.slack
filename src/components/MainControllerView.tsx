import { startWorkspaceRefresh } from "../actions/session";
import { useAuthStore } from "../store/authStore";
import { useUiStore } from "../store/uiStore";
import { BubbleButton } from "./BubbleButton";
import { MessagePopup } from "./MessagePopup";
import { UserPopup } from "./UserPopup";

/**
 * Bottom-center controller bar (see CONTROLER.md). Circle buttons open a
 * bubble popup; rounded-square buttons carry a selected state instead.
 * Log viewer, board editor (gear) and "+" arrive in later milestones.
 */
export function MainControllerView() {
  const avatarUri = useAuthStore((s) => s.avatarUri);
  const activeFilter = useUiStore((s) => s.activeFilter);
  const setActiveFilter = useUiStore((s) => s.setActiveFilter);
  const isRefreshing = useUiStore((s) => s.isRefreshing);

  // ① joined / ② not joined. Pressing the active one again goes back to "All".
  function toggleFilter(target: "JoinedOnly" | "NotJoinedOnly") {
    setActiveFilter(activeFilter === target ? "All" : target);
  }

  return (
    <div className="controller">
      <BubbleButton
        id="user"
        title="ユーザー"
        buttonContent={avatarUri ? <img src={avatarUri} alt="" /> : <span>👤</span>}
        bubbleWidth={280}
      >
        <UserPopup />
      </BubbleButton>

      <BubbleButton id="message" title="メッセージを投稿" buttonContent={<span>✎</span>} bubbleWidth={420}>
        <MessagePopup />
      </BubbleButton>

      <button
        className={isRefreshing ? "square-button square-button--selected" : "square-button"}
        title="データ取得"
        aria-label="データ取得"
        onClick={() => startWorkspaceRefresh()}
        disabled={isRefreshing}
      >
        ↻
      </button>

      <button
        className={activeFilter === "JoinedOnly" ? "square-button square-button--selected" : "square-button"}
        title="参加中のチャンネル"
        onClick={() => toggleFilter("JoinedOnly")}
      >
        ①
      </button>
      <button
        className={activeFilter === "NotJoinedOnly" ? "square-button square-button--selected" : "square-button"}
        title="未参加のチャンネル"
        onClick={() => toggleFilter("NotJoinedOnly")}
      >
        ②
      </button>
    </div>
  );
}
