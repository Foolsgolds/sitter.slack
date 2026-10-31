import { useRef, type ReactNode } from "react";
import {
  arrow,
  FloatingArrow,
  FloatingPortal,
  offset,
  shift,
  useClick,
  useDismiss,
  useFloating,
  useInteractions,
} from "@floating-ui/react";
import { useUiStore, type PopupId } from "../store/uiStore";

interface Props {
  id: PopupId;
  title: string;
  buttonContent: ReactNode;
  bubbleWidth?: number;
  children: ReactNode;
}

/**
 * Circle menu button that toggles a speech-bubble popup above it. Replaces the
 * legacy hand-rolled positioning math: the bubble is centered on the button,
 * shifted to stay inside the window, and the arrow keeps pointing at the
 * button even when the bubble itself had to shift.
 */
export function BubbleButton({ id, title, buttonContent, bubbleWidth = 320, children }: Props) {
  const open = useUiStore((s) => s.openPopup === id);
  const setOpenPopup = useUiStore((s) => s.setOpenPopup);
  const arrowRef = useRef<SVGSVGElement>(null);

  const { refs, floatingStyles, context } = useFloating({
    open,
    onOpenChange: (next) => setOpenPopup(next ? id : null),
    placement: "top",
    middleware: [offset(16), shift({ padding: 8 }), arrow({ element: arrowRef, padding: 20 })],
  });

  const click = useClick(context);
  const dismiss = useDismiss(context);
  const { getReferenceProps, getFloatingProps } = useInteractions([click, dismiss]);

  return (
    <>
      <button
        ref={refs.setReference}
        className="circle-button"
        title={title}
        aria-label={title}
        {...getReferenceProps()}
      >
        {buttonContent}
      </button>
      {open && (
        <FloatingPortal>
          <div
            ref={refs.setFloating}
            className="bubble"
            style={{ ...floatingStyles, width: bubbleWidth }}
            {...getFloatingProps()}
          >
            {/* Scrolling lives on the body so the arrow, which sits outside the box, isn't clipped. */}
            <div className="bubble__body">{children}</div>
            <FloatingArrow
              ref={arrowRef}
              context={context}
              width={20}
              height={12}
              fill="var(--surface)"
              stroke="var(--border)"
              strokeWidth={1}
            />
          </div>
        </FloatingPortal>
      )}
    </>
  );
}
