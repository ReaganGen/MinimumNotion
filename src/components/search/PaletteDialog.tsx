import { Search } from "lucide-react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";

interface PaletteDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  placeholder: string;
  /** Shown until there is something to search. */
  unavailableMessage: string;
}

/**
 * The shell for ⌘K search and ⌘P quick open. Until a workspace can be opened
 * (Phase 2) the input is disabled and the dialog says why.
 */
export function PaletteDialog({
  open,
  onOpenChange,
  title,
  placeholder,
  unavailableMessage,
}: PaletteDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="top-[14vh] max-w-[560px] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-[560px]"
      >
        <DialogTitle className="sr-only">{title}</DialogTitle>
        <div className="flex h-12 items-center gap-3 border-b px-4">
          <Search className="size-4 shrink-0 text-faint-foreground" aria-hidden />
          <input
            disabled
            aria-label={title}
            placeholder={placeholder}
            className="min-w-0 flex-1 bg-transparent text-[15px] outline-none placeholder:text-faint-foreground"
          />
        </div>
        <DialogDescription className="px-4 py-10 text-center text-[13px] text-muted-foreground">
          {unavailableMessage}
        </DialogDescription>
      </DialogContent>
    </Dialog>
  );
}
