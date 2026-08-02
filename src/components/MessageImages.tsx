import { useState } from "react";
import type { MessageImageItem } from "../types/models";
import { fetchImage } from "../api/commands";
import { useUiStore } from "../store/uiStore";

export function MessageImages({ images }: { images: MessageImageItem[] }) {
  if (images.length === 0) return null;
  return (
    <div className="message-images">
      {images.map((image, index) => (
        <LazyImage key={index} candidateUrls={image.candidateUrls} />
      ))}
    </div>
  );
}

/** Slack's private image URLs need a Bearer header a webview can't attach to
 * a plain <img src>, so images are loaded on demand via the `fetch_image`
 * command (which returns a data URL) rather than eagerly. */
function LazyImage({ candidateUrls }: { candidateUrls: string[] }) {
  const [dataUrl, setDataUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const setImagePreview = useUiStore((s) => s.setImagePreview);

  async function load() {
    setLoading(true);
    try {
      const url = await fetchImage(candidateUrls);
      setDataUrl(url);
    } catch (err) {
      console.error("image download failed", err);
      setFailed(true);
    } finally {
      setLoading(false);
    }
  }

  if (failed) return null;

  if (!dataUrl) {
    return (
      <button className="show-image-button" onClick={load} disabled={loading}>
        {loading ? "読み込み中..." : "画像を表示"}
      </button>
    );
  }

  return (
    <img
      className="message-image"
      src={dataUrl}
      alt=""
      onClick={() => setImagePreview(dataUrl)}
    />
  );
}
