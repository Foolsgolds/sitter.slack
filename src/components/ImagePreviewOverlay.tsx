import { useEffect } from "react";
import { useUiStore } from "../store/uiStore";

export function ImagePreviewOverlay() {
  const src = useUiStore((s) => s.imagePreviewSrc);
  const setImagePreview = useUiStore((s) => s.setImagePreview);

  useEffect(() => {
    if (!src) return;
    function handleKey(event: KeyboardEvent) {
      if (event.key === "Escape") setImagePreview(null);
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [src, setImagePreview]);

  if (!src) return null;

  return (
    <div className="image-preview-overlay" onClick={() => setImagePreview(null)}>
      <button className="image-preview-close" onClick={() => setImagePreview(null)}>
        閉じる
      </button>
      {/* eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-noninteractive-element-interactions */}
      <img src={src} alt="" onClick={(event) => event.stopPropagation()} />
    </div>
  );
}
