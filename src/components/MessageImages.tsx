import { useEffect, useState } from "react";
import type { MessageImageItem } from "../types/models";
import { fetchImage, releaseObjectUrl } from "../api/client";
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
 * a plain <img src>, so images are loaded on demand from the server's image
 * proxy (as an object URL) rather than eagerly. */
function LazyImage({ candidateUrls }: { candidateUrls: string[] }) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const setImagePreview = useUiStore((s) => s.setImagePreview);

  // The object URL is released when this image goes away (the full-size preview keeps its own claim).
  useEffect(() => {
    if (!imageUrl) return;
    return () => releaseObjectUrl(imageUrl);
  }, [imageUrl]);

  async function load() {
    setLoading(true);
    try {
      const url = await fetchImage(candidateUrls);
      setImageUrl(url);
    } catch (err) {
      console.error("image download failed", err);
      setFailed(true);
    } finally {
      setLoading(false);
    }
  }

  if (failed) return null;

  if (!imageUrl) {
    return (
      <button className="show-image-button" onClick={load} disabled={loading}>
        {loading ? "読み込み中..." : "画像を表示"}
      </button>
    );
  }

  return (
    <img
      className="message-image"
      src={imageUrl}
      alt=""
      onClick={() => setImagePreview(imageUrl)}
    />
  );
}
