import { useEffect, useMemo, useState } from "react";
import { listAllChannels } from "../api/commands";
import type { ChannelSummary } from "../types/models";

interface Props {
  /** Selected channel IDs, in the order they will appear on the board. */
  selectedIds: string[];
  onChange: (ids: string[]) => void;
}

/**
 * Two-list channel chooser (available / selected). Click a row to move it to
 * the other list. Selection is by channel ID, so a renamed channel stays put.
 */
export function ChannelPicker({ selectedIds, onChange }: Props) {
  const [catalog, setCatalog] = useState<ChannelSummary[] | null>(null);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    let cancelled = false;
    listAllChannels()
      .then((list) => {
        if (!cancelled) setCatalog(list);
      })
      .catch(() => {
        if (!cancelled) setCatalog([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const namesById = useMemo(
    () => new Map((catalog ?? []).map((channel) => [channel.channelId, channel.name])),
    [catalog],
  );

  const available = useMemo(() => {
    const selected = new Set(selectedIds);
    const needle = filter.trim().toLowerCase();
    return (catalog ?? [])
      .filter((channel) => !selected.has(channel.channelId))
      .filter((channel) => needle === "" || channel.name.toLowerCase().includes(needle))
      .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  }, [catalog, selectedIds, filter]);

  return (
    <div className="picker">
      <input
        className="picker__filter"
        value={filter}
        onChange={(event) => setFilter(event.target.value)}
        placeholder="チャンネル名で絞り込み"
        aria-label="チャンネル名で絞り込み"
      />

      <div className="picker__label">追加できるチャンネル</div>
      <div className="picker__list" role="list">
        {catalog === null && <p className="popup-hint">チャンネル一覧を読み込み中です...</p>}
        {catalog !== null && catalog.length === 0 && (
          <p className="popup-hint">チャンネル一覧がありません。データ取得が完了してから開いてください。</p>
        )}
        {available.map((channel) => (
          <button
            key={channel.channelId}
            type="button"
            className="picker__row"
            title="クリックで追加"
            onClick={() => onChange([...selectedIds, channel.channelId])}
          >
            {channel.name}
          </button>
        ))}
      </div>

      <div className="picker__label">選択中のチャンネル ({selectedIds.length})</div>
      <div className="picker__list" role="list">
        {selectedIds.length === 0 && <p className="popup-hint">チャンネルを選択してください。</p>}
        {selectedIds.map((id) => {
          const name = namesById.get(id);
          return (
            <button
              key={id}
              type="button"
              className={name ? "picker__row picker__row--selected" : "picker__row picker__row--missing"}
              title="クリックで外す"
              onClick={() => onChange(selectedIds.filter((selected) => selected !== id))}
            >
              {name ?? `${id} (見つかりません)`}
            </button>
          );
        })}
      </div>
    </div>
  );
}
