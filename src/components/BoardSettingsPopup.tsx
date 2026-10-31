import { useState } from "react";
import { deleteBoard, updateBoard } from "../actions/boards";
import { useBoardStore } from "../store/boardStore";
import { useUiStore } from "../store/uiStore";
import { BoardForm } from "./BoardForm";

/** Settings (rename / change channels / delete) for the custom board on screen. */
export function BoardSettingsPopup() {
  const activeFilter = useUiStore((s) => s.activeFilter);
  const setOpenPopup = useUiStore((s) => s.setOpenPopup);
  const index = useBoardStore((s) => s.activeCustomBoardIndex);
  const board = useBoardStore((s) => s.customBoards[s.activeCustomBoardIndex]);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  if (activeFilter !== "CustomOnly" || !board) {
    return <p className="popup-hint">カスタムボードを選択すると、名前やチャンネルを変更できます。</p>;
  }

  return (
    <BoardForm
      // Re-seed the form if the active board changes while this is open.
      key={`${index}:${board.name}:${board.channelIds.join(",")}`}
      idPrefix="board-settings"
      initialName={board.name}
      initialChannelIds={board.channelIds}
      submitLabel="変更"
      requireChange
      onSubmit={(name, channelIds) => updateBoard(index, name, channelIds)}
      onCancel={() => setOpenPopup(null)}
      extraActions={
        <button
          type="button"
          className="danger"
          onClick={() => {
            if (confirmingDelete) deleteBoard(index);
            else setConfirmingDelete(true);
          }}
        >
          {confirmingDelete ? "本当に削除する" : "ボードを削除"}
        </button>
      }
    />
  );
}
