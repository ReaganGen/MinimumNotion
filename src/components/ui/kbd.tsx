import type * as React from "react";
import { cn } from "@/lib/utils";

/** A keyboard shortcut label, e.g. <Kbd>⌘K</Kbd>. */
export function Kbd({ className, ...props }: React.ComponentProps<"kbd">) {
  return (
    <kbd
      className={cn(
        "inline-flex h-5 items-center font-sans text-[11px] tracking-wider text-faint-foreground",
        className,
      )}
      {...props}
    />
  );
}
