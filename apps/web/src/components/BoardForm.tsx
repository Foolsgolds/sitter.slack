import { useState, type FormEvent, type ReactNode } from "react";
import { DEFAULT_BOARD_NAME } from "../constants";
import { ChannelPicker } from "./ChannelPicker";

interface Props {
  idPrefix: string;
  initialName: string;
  initialChannelIds: string[];
  submitLabel: string;
  /** Keep the submit button disabled until the name or channels differ from the initial values. */
  requireChange?: boolean;
  /** When set, shown instead of allowing a submit (e.g. the board limit). */
  blockedNotice?: string;
  onSubmit: (name: string, channelIds: string[]) => void;
  onCancel: () => void;
  extraActions?: ReactNode;
}

/** Name + channel chooser shared by the "add board" and "board settings" popups. */
export function BoardForm({
  idPrefix,
  initialName,
  initialChannelIds,
  submitLabel,
  requireChange = false,
  blockedNotice,
  onSubmit,
  onCancel,
  extraActions,
}: Props) {
  const [name, setName] = useState(initialName);
  const [channelIds, setChannelIds] = useState(initialChannelIds);

  const changed =
    name.trim() !== initialName.trim() ||
    channelIds.length !== initialChannelIds.length ||
    channelIds.some((id, i) => id !== initialChannelIds[i]);
  const canSubmit = !blockedNotice && channelIds.length > 0 && (!requireChange || changed);

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (canSubmit) onSubmit(name, channelIds);
  }

  return (
    <form className="popup-form" onSubmit={handleSubmit}>
      <label htmlFor={`${idPrefix}-name`}>ボード名</label>
      <input
        id={`${idPrefix}-name`}
        value={name}
        onChange={(event) => setName(event.target.value)}
        placeholder={DEFAULT_BOARD_NAME}
      />

      <ChannelPicker selectedIds={channelIds} onChange={setChannelIds} />

      {blockedNotice && <p className="popup-status popup-status--error">{blockedNotice}</p>}

      <div className="popup-actions">
        {extraActions}
        <button type="button" onClick={onCancel}>
          キャンセル
        </button>
        <button type="submit" className="primary" disabled={!canSubmit}>
          {submitLabel}
        </button>
      </div>
    </form>
  );
}
