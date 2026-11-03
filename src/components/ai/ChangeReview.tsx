// Reviewing what the assistant proposed (ARCHITECTURE §10.3). Nothing has
// been written when this shows: every change is listed with what it does
// to the page, word by word, and only "Apply" here writes anything. Text
// from notes and from the model is shown as text; images in it are shown
// as their addresses, and new external ones are flagged (ADR-031).
import {
  ArrowRight,
  FilePlus2,
  FileText,
  FolderInput,
  ListChecks,
  RotateCcw,
  Table2,
  TriangleAlert,
  Undo2,
} from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { type ChatTurn, useAiChatStore } from "@/features/ai/aiStore";
import { collapseDiff, dependents } from "@/features/ai/diff";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { ChangeConflict } from "@/lib/ipc/bindings/ChangeConflict";
import type { ChangeKind } from "@/lib/ipc/bindings/ChangeKind";
import type { ChangePreview } from "@/lib/ipc/bindings/ChangePreview";
import type { DiffSpan } from "@/lib/ipc/bindings/DiffSpan";
import { cn } from "@/lib/utils";

const ICONS: Record<ChangeKind, typeof FileText> = {
  create_page: FilePlus2,
  create_collection: Table2,
  edit_page: FileText,
  set_properties: ListChecks,
  move_page: FolderInput,
};

function count(n: number, one: string, many: string) {
  return `${n} ${n === 1 ? one : many}`;
}

/** Under an answer in Edit mode: what it proposes, and its state. */
export function ProposalCard({ turn, onAskAgain }: { turn: ChatTurn; onAskAgain: () => void }) {
  const [reviewing, setReviewing] = useState(false);
  const { rejectProposal, undoProposal } = useAiChatStore.getState();
  const proposal = turn.proposal;
  if (!proposal) return null;
  const { preview, status } = proposal;
  const changes = preview.changes;
  const openPage = useWorkspaceStore.getState().openPage;

  return (
    <section
      aria-label="Proposed changes"
      className="mt-2.5 rounded-lg px-3 py-2.5 text-[12.5px] shadow-popover"
    >
      <p className="font-medium text-foreground">
        {status === "applied"
          ? proposal.undo === "undone"
            ? "Changes undone"
            : "Changes applied"
          : status === "rejected"
            ? "Changes rejected"
            : `Proposed ${count(changes.length, "change", "changes")}`}
      </p>
      {status !== "applied" && status !== "rejected" && (
        <p className="mt-0.5 text-muted-foreground">Nothing has changed yet. Review them first.</p>
      )}
      <ul className="mt-1.5 flex flex-col gap-0.5">
        {changes.slice(0, 6).map((change) => {
          const Icon = ICONS[change.kind];
          return (
            <li key={change.change_id} className="flex items-start gap-1.5 text-muted-foreground">
              <Icon className="mt-0.5 size-3.5 shrink-0" aria-hidden />
              <span className="min-w-0">{change.summary}</span>
            </li>
          );
        })}
        {changes.length > 6 && (
          <li className="pl-5 text-faint-foreground">and {changes.length - 6} more</li>
        )}
      </ul>

      {status === "conflicts" && proposal.conflicts && (
        <Conflicts conflicts={proposal.conflicts} lead="Nothing was applied:" />
      )}
      {proposal.error && <p className="mt-1.5 text-destructive">{proposal.error}</p>}

      {status === "applied" && proposal.operation && (
        <div className="mt-1.5 text-muted-foreground">
          <p>{proposal.operation.summary}.</p>
          <p className="mt-1 flex flex-wrap gap-x-2">
            {proposal.operation.pages
              .filter((p) => p.title !== null)
              .slice(0, 6)
              .map((p) => (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => openPage(p.id)}
                  className="text-link hover:underline"
                >
                  {p.title}
                </button>
              ))}
          </p>
          {typeof proposal.undo === "object" && "conflicts" in proposal.undo && (
            <Conflicts conflicts={proposal.undo.conflicts} lead="Nothing was undone:" />
          )}
          {typeof proposal.undo === "object" && "error" in proposal.undo && (
            <p className="mt-1 text-destructive">{proposal.undo.error}</p>
          )}
        </div>
      )}

      <div className="mt-2 flex flex-wrap gap-1.5">
        {(status === "pending" || status === "conflicts" || status === "applying") && (
          <>
            <CardButton primary onClick={() => setReviewing(true)} disabled={status === "applying"}>
              Review changes
            </CardButton>
            <CardButton onClick={() => rejectProposal(turn.id)} disabled={status === "applying"}>
              Reject
            </CardButton>
          </>
        )}
        {status === "conflicts" && (
          <CardButton onClick={onAskAgain}>
            <RotateCcw className="size-3.5" aria-hidden /> Ask for a new proposal
          </CardButton>
        )}
        {status === "applied" && proposal.undo !== "undone" && (
          <CardButton
            onClick={() => void undoProposal(turn.id)}
            disabled={proposal.undo === "undoing"}
          >
            <Undo2 className="size-3.5" aria-hidden /> Undo
          </CardButton>
        )}
      </div>
      <ReviewDialog turn={turn} open={reviewing} onOpenChange={setReviewing} />
    </section>
  );
}

function CardButton({
  primary,
  onClick,
  disabled,
  children,
}: {
  primary?: boolean;
  onClick: () => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "inline-flex items-center gap-1 rounded-md px-2 py-1 text-[12.5px] disabled:opacity-50",
        primary
          ? "bg-primary text-primary-foreground"
          : "text-muted-foreground hover:bg-accent hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

function Conflicts({ conflicts, lead }: { conflicts: ChangeConflict[]; lead: string }) {
  return (
    <div role="alert" className="mt-1.5 rounded-md bg-muted px-2 py-1.5 text-muted-foreground">
      <p className="flex items-center gap-1 text-foreground">
        <TriangleAlert className="size-3.5 shrink-0 text-destructive" aria-hidden /> {lead}
      </p>
      <ul className="mt-0.5 flex flex-col gap-0.5">
        {conflicts.map((c) => (
          <li key={`${c.change_id}-${c.message}`}>
            “{c.title}”: {c.message}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The full review: choose changes, see each one, apply or reject. */
export function ReviewDialog({
  turn,
  open,
  onOpenChange,
}: {
  turn: ChatTurn;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const proposal = turn.proposal;
  const changes = proposal?.preview.changes ?? [];
  const [chosen, setChosen] = useState<Set<string>>(() => new Set(changes.map((c) => c.change_id)));
  const { applyProposal, rejectProposal } = useAiChatStore.getState();
  const conflicted = useMemo(
    () => new Set((proposal?.conflicts ?? []).map((c) => c.change_id)),
    [proposal?.conflicts],
  );
  if (!proposal) return null;

  function toggle(id: string) {
    const { needs, neededBy } = dependents(changes, id);
    setChosen((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        // What goes inside a page can't come without it.
        for (const other of [id, ...neededBy]) next.delete(other);
      } else {
        for (const other of [id, ...needs]) next.add(other);
      }
      return next;
    });
  }

  async function apply() {
    // In the order proposed.
    await applyProposal(
      turn.id,
      changes.map((c) => c.change_id).filter((id) => chosen.has(id)),
    );
    const status = useAiChatStore.getState().turns.find((t) => t.id === turn.id)?.proposal?.status;
    if (status === "applied") onOpenChange(false);
  }

  const busy = proposal.status === "applying";
  const images = changes.flatMap((c) => c.external_images);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[82vh] max-w-[720px] flex-col gap-0 overflow-hidden p-0 sm:max-w-[720px]">
        <div className="border-b px-5 py-4">
          <DialogTitle className="text-[15px] font-semibold">Review proposed changes</DialogTitle>
          <DialogDescription className="mt-1 text-[13px] text-muted-foreground">
            For “{proposal.preview.request}”. Nothing changes until you apply. You can undo it
            afterwards.
          </DialogDescription>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3">
          {images.length > 0 && (
            <div className="mb-3 flex gap-2 rounded-md bg-muted px-3 py-2 text-[12.5px] leading-5 text-muted-foreground">
              <TriangleAlert className="mt-0.5 size-4 shrink-0 text-destructive" aria-hidden />
              <div className="min-w-0">
                <p className="text-foreground">
                  These changes add images from other websites. Once applied, opening the page loads
                  them, and the website sees that it was opened:
                </p>
                <ul className="mt-0.5 break-all">
                  {images.map((url) => (
                    <li key={url}>{url}</li>
                  ))}
                </ul>
              </div>
            </div>
          )}
          {proposal.status === "conflicts" && proposal.conflicts && (
            <Conflicts
              conflicts={proposal.conflicts}
              lead="Some pages changed since this was proposed, so nothing was applied. Leave those changes out, or ask for a new proposal."
            />
          )}
          <ol className="flex flex-col gap-3">
            {changes.map((change) => (
              <ChangeItem
                key={change.change_id}
                change={change}
                chosen={chosen.has(change.change_id)}
                conflicted={conflicted.has(change.change_id)}
                onToggle={() => toggle(change.change_id)}
              />
            ))}
          </ol>
        </div>
        <div className="flex items-center gap-2 border-t px-5 py-3">
          {proposal.error && (
            <p role="alert" className="min-w-0 flex-1 text-[12.5px] text-destructive">
              {proposal.error}
            </p>
          )}
          <div className="ml-auto flex gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={() => {
                rejectProposal(turn.id);
                onOpenChange(false);
              }}
              className="rounded-md px-3 py-1.5 text-[13px] text-muted-foreground hover:bg-accent disabled:opacity-50"
            >
              Reject all
            </button>
            <button
              type="button"
              disabled={busy || chosen.size === 0}
              onClick={() => void apply()}
              className="rounded-md bg-primary px-3 py-1.5 text-[13px] text-primary-foreground disabled:opacity-50"
            >
              {busy
                ? "Applying…"
                : chosen.size === changes.length
                  ? `Apply ${count(changes.length, "change", "changes")}`
                  : `Apply ${chosen.size} of ${changes.length}`}
            </button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Path({ path }: { path: string[] }) {
  return <span>{path.length > 0 ? path.join(" › ") : "Top level"}</span>;
}

function ChangeItem({
  change,
  chosen,
  conflicted,
  onToggle,
}: {
  change: ChangePreview;
  chosen: boolean;
  conflicted: boolean;
  onToggle: () => void;
}) {
  const Icon = ICONS[change.kind];
  const id = `change-${change.change_id}`;
  return (
    <li className={cn("rounded-lg border px-3 py-2.5", !chosen && "opacity-60")}>
      <div className="flex items-start gap-2">
        <input id={id} type="checkbox" checked={chosen} onChange={onToggle} className="mt-0.5" />
        <label htmlFor={id} className="min-w-0 flex-1 cursor-default">
          <span className="flex items-center gap-1.5 text-[13px] font-medium text-foreground">
            <Icon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
            {change.summary}
          </span>
          <span className="mt-0.5 block text-[12px] text-faint-foreground">
            {change.kind === "move_page" && change.new_location ? (
              <span className="inline-flex flex-wrap items-center gap-1">
                <Path path={change.location} />
                <ArrowRight className="size-3" aria-hidden />
                <Path path={change.new_location} />
              </span>
            ) : (
              <Path path={change.location} />
            )}
            {change.depends_on.length > 0 && ` · needs change ${change.depends_on.join(", ")}`}
          </span>
        </label>
      </div>
      {conflicted && (
        <p className="mt-1.5 text-[12px] text-destructive">This page changed since the proposal.</p>
      )}
      {change.details.length > 0 && (
        <ul className="mt-1.5 list-disc pl-9 text-[12px] text-muted-foreground">
          {change.details.map((d) => (
            <li key={d}>{d}</li>
          ))}
        </ul>
      )}
      {change.properties.length > 0 && (
        <table className="mt-1.5 ml-6 text-[12px]">
          <tbody>
            {change.properties.map((p) => (
              <tr key={p.name}>
                <td className="pr-3 text-muted-foreground">{p.name}</td>
                <td>
                  {p.before !== null && (
                    <span className="text-muted-foreground line-through">{p.before}</span>
                  )}
                  {p.before !== null && " → "}
                  <span>{p.after ?? <em className="text-faint-foreground">empty</em>}</span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {change.diff.length > 0 && <DiffView spans={change.diff} />}
    </li>
  );
}

/** Word-level changes; the text is shown as written, never rendered. */
export function DiffView({ spans }: { spans: DiffSpan[] }) {
  const parts = collapseDiff(spans);
  return (
    <div className="selectable mt-2 ml-6 max-h-72 overflow-y-auto rounded-md bg-muted px-3 py-2 font-mono text-[12px] leading-5 whitespace-pre-wrap break-words">
      {parts.map((part, i) =>
        part.kind === "gap" ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts are positional
          <span key={i} className="text-faint-foreground">
            {" "}
            … {part.hidden} unchanged characters …{" "}
          </span>
        ) : part.kind === "insert" ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts are positional
          <ins key={i} className="rounded-sm bg-diff-insert no-underline">
            {part.text}
          </ins>
        ) : part.kind === "delete" ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts are positional
          <del key={i} className="rounded-sm bg-diff-delete text-muted-foreground">
            {part.text}
          </del>
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts are positional
          <span key={i}>{part.text}</span>
        ),
      )}
    </div>
  );
}
