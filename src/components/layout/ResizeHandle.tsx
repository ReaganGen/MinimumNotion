import type * as React from "react";
import { useRef } from "react";
import { cn } from "@/lib/utils";

const KEYBOARD_STEP = 16;

interface ResizeHandleProps {
  /** The panel edge the handle sits on. A handle on the right edge grows the panel when dragged right. */
  edge: "left" | "right";
  label: string;
  width: number;
  min: number;
  max: number;
  defaultWidth: number;
  onResize: (width: number) => void;
  onDraggingChange: (dragging: boolean) => void;
}

/** A vertical separator that resizes its panel by pointer or keyboard. Double-click resets the width. */
export function ResizeHandle({
  edge,
  label,
  width,
  min,
  max,
  defaultWidth,
  onResize,
  onDraggingChange,
}: ResizeHandleProps) {
  const drag = useRef<{ startX: number; startWidth: number } | null>(null);
  const direction = edge === "right" ? 1 : -1;

  function onPointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { startX: event.clientX, startWidth: width };
    onDraggingChange(true);
  }

  function onPointerMove(event: React.PointerEvent<HTMLDivElement>) {
    if (!drag.current) return;
    onResize(drag.current.startWidth + (event.clientX - drag.current.startX) * direction);
  }

  function endDrag(event: React.PointerEvent<HTMLDivElement>) {
    if (!drag.current) return;
    drag.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    onDraggingChange(false);
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    const next =
      event.key === "ArrowRight"
        ? width + KEYBOARD_STEP * direction
        : event.key === "ArrowLeft"
          ? width - KEYBOARD_STEP * direction
          : event.key === "Home"
            ? min
            : event.key === "End"
              ? max
              : null;
    if (next === null) return;
    event.preventDefault();
    onResize(next);
  }

  return (
    // biome-ignore lint/a11y/useSemanticElements: <hr> can't be focusable or carry pointer handlers; a focusable separator is the WAI-ARIA window-splitter pattern.
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={width}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onDoubleClick={() => onResize(defaultWidth)}
      onKeyDown={onKeyDown}
      className={cn(
        "group absolute inset-y-0 z-10 flex w-2 cursor-col-resize justify-center outline-none",
        edge === "right" ? "right-0" : "left-0",
      )}
    >
      <div className="h-full w-0.5 bg-transparent transition-colors duration-(--duration-fast) group-hover:bg-border group-focus-visible:bg-ring group-active:bg-ring" />
    </div>
  );
}
