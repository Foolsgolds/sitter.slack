import { useEffect, useState, type FormEvent } from "react";
import { listAllChannels, postMessage, refreshChannel } from "../api/commands";
import { useChannelsStore } from "../store/channelsStore";
import { useUiStore } from "../store/uiStore";
import type { ChannelSummary } from "../types/models";

const LAST_CHANNEL_KEY = "slacksitter.lastPostChannel";

function loadLastChannel(): string {
  try {
    return localStorage.getItem(LAST_CHANNEL_KEY) ?? "";
  } catch {
    return "";
  }
}

function saveLastChannel(name: string) {
  try {
    localStorage.setItem(LAST_CHANNEL_KEY, name);
  } catch {
    // Remembering the last channel is only a convenience.
  }
}

type PostStatus = { kind: "error" | "busy"; text: string } | null;

export function MessagePopup() {
  const [channelName, setChannelName] = useState(loadLastChannel);
  const [body, setBody] = useState("");
  const [channels, setChannels] = useState<ChannelSummary[]>([]);
  const [status, setStatus] = useState<PostStatus>(null);
  const setOpenPopup = useUiStore((s) => s.setOpenPopup);
  const upsertOne = useChannelsStore((s) => s.upsertOne);

  useEffect(() => {
    let cancelled = false;
    listAllChannels()
      .then((list) => {
        if (!cancelled) setChannels(list);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    const name = channelName.trim().replace(/^#/, "");
    if (!name) {
      setStatus({ kind: "error", text: "チャンネル名を入力してください" });
      return;
    }
    if (!body.trim()) {
      setStatus({ kind: "error", text: "コメントを入力してください" });
      return;
    }

    setStatus({ kind: "busy", text: "投稿中..." });
    try {
      await postMessage(name, body);
      saveLastChannel(name);

      // Show the new post right away if that channel is on a board.
      const posted = Array.from(useChannelsStore.getState().channels.values()).find(
        (channel) => channel.name.toLowerCase() === name.toLowerCase(),
      );
      if (posted) {
        refreshChannel(posted.channelId)
          .then(upsertOne)
          .catch(() => {});
      }

      setBody("");
      setStatus(null);
      setOpenPopup(null);
    } catch (err) {
      setStatus({ kind: "error", text: `投稿に失敗しました: ${String(err)}` });
    }
  }

  const busy = status?.kind === "busy";

  // A stale validation/post error shouldn't linger once the user edits again.
  function clearError() {
    setStatus((current) => (current?.kind === "error" ? null : current));
  }

  return (
    <form className="popup-form" onSubmit={handleSubmit}>
      <label htmlFor="post-channel">チャンネル名</label>
      <input
        id="post-channel"
        list="post-channel-options"
        value={channelName}
        onChange={(event) => {
          setChannelName(event.target.value);
          clearError();
        }}
        placeholder="times-xxxx"
        disabled={busy}
      />
      <datalist id="post-channel-options">
        {channels.map((channel) => (
          <option key={channel.channelId} value={channel.name} />
        ))}
      </datalist>

      <label htmlFor="post-body">コメント</label>
      <textarea
        id="post-body"
        value={body}
        onChange={(event) => {
          setBody(event.target.value);
          clearError();
        }}
        rows={5}
        placeholder="コメントを入力"
        disabled={busy}
      />

      {status && (
        <p className={status.kind === "error" ? "popup-status popup-status--error" : "popup-status"}>
          {status.text}
        </p>
      )}

      <div className="popup-actions">
        <button type="button" onClick={() => setOpenPopup(null)} disabled={busy}>
          キャンセル
        </button>
        <button type="submit" className="primary" disabled={busy}>
          コメント
        </button>
      </div>
    </form>
  );
}
