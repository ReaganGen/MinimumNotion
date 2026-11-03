import { Info, X } from "lucide-react";
import { useSettingsStore } from "@/features/settings/settingsStore";

/** A quiet, dismissible message about settings (reset, read-only, or a failed save). */
export function NoticeBar() {
  const message = useSettingsStore((s) => s.saveError ?? s.notice);
  const dismiss = useSettingsStore((s) => s.dismissNotice);
  if (!message) return null;

  return (
    <div
      role="status"
      className="mx-3 mb-1 flex items-start gap-2 rounded-md bg-muted px-3 py-2 text-[13px] text-muted-foreground"
    >
      <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
      <p className="selectable min-w-0 flex-1 leading-5">{message}</p>
      <button
        type="button"
        aria-label="Dismiss"
        onClick={dismiss}
        className="rounded-sm p-0.5 hover:bg-accent hover:text-foreground"
      >
        <X className="size-3.5" aria-hidden />
      </button>
    </div>
  );
}
