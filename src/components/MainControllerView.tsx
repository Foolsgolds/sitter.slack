import { toggleCustomBoard } from "../actions/boards";
import { refreshActiveBoard } from "../actions/session";
import { useAuthStore } from "../store/authStore";
import { useBoardStore } from "../store/boardStore";
import { useUiStore } from "../store/uiStore";
import { BoardAddPopup } from "./BoardAddPopup";
import { BoardSettingsPopup } from "./BoardSettingsPopup";
import { BubbleButton } from "./BubbleButton";
import { LogPopup } from "./LogPopup";
import { MessagePopup } from "./MessagePopup";
import { UserPopup } from "./UserPopup";

/** The first character (code point) of a board name labels its button. */
function boardInitial(name: string): string {
  return Array.from(name.trim())[0] ?? "?";
}

/**
 * Bottom-center controller bar (see CONTROLER.md). Circle buttons open a
 * bubble popup; rounded-square buttons carry a selected state instead.
 * Order follows the legacy bar: log, user, gear, message, plus, refresh, ①, ②,
 * then one button per custom board.
 */
export function MainControllerView() {
  const avatarUri = useAuthStore((s) => s.avatarUri);
  const activeFilter = useUiStore((s) => s.activeFilter);
  const setActiveFilter = useUiStore((s) => s.setActiveFilter);
  const isRefreshing = useUiStore((s) => s.isRefreshing);
  const customBoards = useBoardStore((s) => s.customBoards);
  const activeCustomBoardIndex = useBoardStore((s) => s.activeCustomBoardIndex);

  // ① joined / ② not joined. Pressing the active one again goes back to "All".
  function toggleFilter(target: "JoinedOnly" | "NotJoinedOnly") {
    setActiveFilter(activeFilter === target ? "All" : target);
  }

  return (
    <div className="controller">
      <BubbleButton id="log" title="ログ" buttonContent={<span>📋</span>} bubbleWidth={560}>
        <LogPopup />
      </BubbleButton>

      <BubbleButton
        id="user"
        title="ユーザー"
        buttonContent={avatarUri ? <img src={avatarUri} alt="" /> : <span>👤</span>}
        bubbleWidth={280}
      >
        <UserPopup />
      </BubbleButton>

      <BubbleButton id="boardSettings" title="ボードの設定" buttonContent={<span>⚙</span>} bubbleWidth={440}>
        <BoardSettingsPopup />
      </BubbleButton>

      <BubbleButton id="message" title="メッセージを投稿" buttonContent={<span>✎</span>} bubbleWidth={420}>
        <MessagePopup />
      </BubbleButton>

      <BubbleButton id="boardAdd" title="ボードを追加" buttonContent={<span>＋</span>} bubbleWidth={440}>
        <BoardAddPopup />
      </BubbleButton>

      <button
        className={isRefreshing ? "square-button square-button--selected" : "square-button"}
        title="表示中のボードを再取得"
        aria-label="表示中のボードを再取得"
        onClick={() => void refreshActiveBoard()}
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

      {customBoards.map((board, index) => (
        <button
          key={index}
          className={
            activeFilter === "CustomOnly" && activeCustomBoardIndex === index
              ? "square-button square-button--selected"
              : "square-button"
          }
          title={board.name}
          aria-label={board.name}
          onClick={() => toggleCustomBoard(index)}
        >
          {boardInitial(board.name)}
        </button>
      ))}
    </div>
  );
}
