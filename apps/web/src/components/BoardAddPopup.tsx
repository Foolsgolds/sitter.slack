import { addBoard } from "../actions/boards";
import { MAX_CUSTOM_BOARDS } from "../constants";
import { useBoardStore } from "../store/boardStore";
import { useUiStore } from "../store/uiStore";
import { BoardForm } from "./BoardForm";

export function BoardAddPopup() {
  const boardCount = useBoardStore((s) => s.customBoards.length);
  const setOpenPopup = useUiStore((s) => s.setOpenPopup);

  return (
    <BoardForm
      idPrefix="board-add"
      initialName=""
      initialChannelIds={[]}
      submitLabel="追加"
      blockedNotice={
        boardCount >= MAX_CUSTOM_BOARDS ? `追加できるカスタムボードは最大 ${MAX_CUSTOM_BOARDS} 件です` : undefined
      }
      onSubmit={(name, channelIds) => addBoard(name, channelIds)}
      onCancel={() => setOpenPopup(null)}
    />
  );
}
