import { Info, X } from "lucide-react";
import { useSettingsStore } from "@/features/settings/settingsStore";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

/** Quiet, dismissible messages about settings and the workspace. */
export function NoticeBar() {
  const settingsMessage = useSettingsStore((s) => s.saveError ?? s.notice);
  const dismissSettings = useSettingsStore((s) => s.dismissNotice);
  const workspaceNotices = useWorkspaceStore((s) => s.notices);
  const dismissWorkspace = useWorkspaceStore((s) => s.dismissNotice);

  return (
    <>
      {settingsMessage && <Notice message={settingsMessage} onDismiss={dismissSettings} />}
      {workspaceNotices.map((message) => (
        <Notice key={message} message={message} onDismiss={() => dismissWorkspace(message)} />
      ))}
    </>
  );
}

function Notice({ message, onDismiss }: { message: string; onDismiss: () => void }) {
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
        onClick={onDismiss}
        className="rounded-sm p-0.5 hover:bg-accent hover:text-foreground"
      >
        <X className="size-3.5" aria-hidden />
      </button>
    </div>
  );
}
