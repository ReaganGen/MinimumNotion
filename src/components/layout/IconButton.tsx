import type * as React from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

interface IconButtonProps extends React.ComponentProps<"button"> {
  /** Accessible name, also shown in the tooltip. */
  label: string;
  shortcut?: string;
  tooltipSide?: "top" | "bottom" | "left" | "right";
}

/** A small icon-only button with a tooltip, used in panel headers. */
export function IconButton({
  label,
  shortcut,
  tooltipSide = "bottom",
  className,
  children,
  ...props
}: IconButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          className={cn(
            "inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground",
            "transition-colors duration-(--duration-fast) hover:bg-accent hover:text-foreground",
            "[&_svg]:size-4 [&_svg]:shrink-0",
            className,
          )}
          {...props}
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent side={tooltipSide}>
        {label}
        {shortcut && <span className="ml-2 opacity-60">{shortcut}</span>}
      </TooltipContent>
    </Tooltip>
  );
}
