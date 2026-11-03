import type * as React from "react";
import type { PanelLayout } from "@/features/layout/layout";
import { cn } from "@/lib/utils";

interface SidePanelProps {
  side: "left" | "right";
  label: string;
  layout: PanelLayout;
  /** Disables the width animation while the user drags the resize handle. */
  resizing: boolean;
  className?: string;
  children: React.ReactNode;
}

/**
 * A collapsible panel beside the editor. Docked panels animate their width so
 * the editor reflows smoothly. Overlay panels (used when the window is too
 * narrow) slide over the editor. Hidden panels are `inert`, so keyboard focus
 * can't land inside them.
 */
export function SidePanel({ side, label, layout, resizing, className, children }: SidePanelProps) {
  const docked = layout.mode === "docked";
  const offscreen = side === "left" ? "-100%" : "100%";
  const style: React.CSSProperties = docked
    ? { width: layout.visible ? layout.width : 0 }
    : { width: layout.width, transform: layout.visible ? "none" : `translateX(${offscreen})` };

  return (
    <aside
      aria-label={label}
      inert={!layout.visible}
      data-state={layout.visible ? "open" : "closed"}
      data-mode={layout.mode}
      style={style}
      className={cn(
        "shrink-0 overflow-hidden",
        docked ? "relative" : "absolute inset-y-0 z-30 shadow-popover",
        side === "left" ? "left-0" : "right-0",
        layout.visible && (side === "left" ? "border-r" : "border-l"),
        !resizing &&
          "transition-[width,transform] duration-(--duration-panel) ease-(--ease-standard)",
        className,
      )}
    >
      <div className="relative h-full" style={{ width: layout.width }}>
        {children}
      </div>
    </aside>
  );
}
