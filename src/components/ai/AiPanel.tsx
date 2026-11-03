import {
  ArrowUp,
  BookOpen,
  Cloud,
  FileText,
  Laptop,
  RotateCcw,
  Search,
  Sparkles,
  Square,
  TextQuote,
  Trash2,
  TriangleAlert,
  X,
} from "lucide-react";
import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { IconButton } from "@/components/layout/IconButton";
import { askAboutPage, openSource, summarizePage } from "@/features/ai/actions";
import { type ChatTurn, useAiChatStore } from "@/features/ai/aiStore";
import { checkCitations } from "@/features/ai/citations";
import { destinationLabel, hostOf, isLocalUrl, readiness } from "@/features/ai/providers";
import { runCommand, SHORTCUTS } from "@/features/commands/commands";
import { useSettingsStore } from "@/features/settings/settingsStore";
import { useUiStore } from "@/features/ui/uiStore";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { ContextReport } from "@/lib/ipc/bindings/ContextReport";
import type { ProviderConfig } from "@/lib/ipc/bindings/ProviderConfig";
import type { Source } from "@/lib/ipc/bindings/Source";
import { ipc } from "@/lib/ipc/client";
import { cn } from "@/lib/utils";
import { ChatMarkdown } from "./ChatMarkdown";

/** The AI assistant panel: chat with the active provider about your notes. */
export function AiPanel() {
  const ai = useSettingsStore((s) => s.settings.ai);
  const turns = useAiChatStore((s) => s.turns);
  const clear = useAiChatStore((s) => s.clear);
  const ready = readiness(ai);

  return (
    <div className="flex h-full flex-col bg-background">
      <div
        data-tauri-drag-region
        className="flex h-(--titlebar-height) shrink-0 items-center justify-between pr-2 pl-4"
      >
        <h2 data-tauri-drag-region className="text-[13px] font-medium text-foreground">
          AI assistant
        </h2>
        <div className="flex items-center">
          {turns.length > 0 && (
            <IconButton label="Clear the conversation" onClick={clear}>
              <Trash2 />
            </IconButton>
          )}
          <IconButton
            label="Close AI assistant"
            shortcut={SHORTCUTS["toggle-ai-panel"].keys}
            onClick={() => runCommand("toggle-ai-panel")}
          >
            <X />
          </IconButton>
        </div>
      </div>

      {ready.state === "disabled" && (
        <EmptyState
          title="AI is off"
          action="Turn on in Settings"
          body="Forma works fully without it. Turn it on to ask about your notes with OpenAI, Anthropic, or a local model through Ollama or LM Studio."
        />
      )}
      {ready.state === "no-provider" && (
        <EmptyState
          title="No provider yet"
          action="Set up a provider"
          body="Choose a cloud provider or a model running on this Mac."
        />
      )}
      {ready.state === "no-model" && (
        <EmptyState
          title="Choose a model"
          action="Open AI settings"
          body={`${ready.provider.name} is set up, but no model is chosen.`}
        />
      )}
      {ready.state === "no-key" && (
        <EmptyState
          title="Add an API key"
          action="Open AI settings"
          body={`${ready.provider.name} needs an API key. Forma keeps it in your Keychain.`}
        />
      )}
      {(ready.state === "ready" || ready.state === "needs-agreement") && (
        <Conversation provider={ready.provider} agreed={ready.state === "ready"} />
      )}
    </div>
  );
}

function EmptyState({ title, body, action }: { title: string; body: string; action: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-10 pb-16 text-center">
      <Sparkles className="size-5 text-faint-foreground" aria-hidden />
      <p className="mt-3 text-[13px] font-medium text-foreground">{title}</p>
      <p className="mt-1 text-[13px] leading-5 text-muted-foreground">{body}</p>
      <button
        type="button"
        onClick={() => useUiStore.getState().openSettings("ai")}
        className="mt-4 rounded-md px-3 py-1.5 text-[13px] text-foreground shadow-popover hover:bg-accent"
      >
        {action}
      </button>
    </div>
  );
}

/** The open page, as the panel's actions need it. */
function useOpenPage(): { id: string; title: string } | null {
  const id = useWorkspaceStore((s) => s.activePageId);
  const title = useWorkspaceStore((s) => s.breadcrumbs.at(-1)?.title);
  return id && title !== undefined ? { id, title } : null;
}

function Conversation({ provider, agreed }: { provider: ProviderConfig; agreed: boolean }) {
  const turns = useAiChatStore((s) => s.turns);
  const busy = useAiChatStore((s) => s.requestId !== null);
  const hasWorkspace = useWorkspaceStore((s) => s.info !== null);
  const openPage = useOpenPage();
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  // Follows the answer as it streams, unless the user scrolled up to read.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs whenever the turns change
  useEffect(() => {
    const el = scroller.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [turns]);

  return (
    <>
      <div
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
        className="min-h-0 flex-1 overflow-y-auto px-4 pb-4"
      >
        {turns.length === 0 ? (
          <div className="mt-6 text-[13px] leading-5 text-muted-foreground">
            {hasWorkspace ? (
              <p>Ask about your notes. Answers cite the passages they draw on.</p>
            ) : (
              <p>
                Open a workspace to ask about your notes. Until then, the assistant only knows what
                you write here.
              </p>
            )}
            {openPage && agreed && (
              <div className="mt-3 flex flex-col items-start gap-1">
                <SuggestionButton onClick={() => summarizePage(openPage)}>
                  <BookOpen aria-hidden /> Summarize this page
                </SuggestionButton>
                <SuggestionButton onClick={() => askAboutPage(openPage)}>
                  <FileText aria-hidden /> Ask about this page
                </SuggestionButton>
              </div>
            )}
          </div>
        ) : (
          <ol aria-label="Conversation" className="flex flex-col gap-4 pt-2">
            {turns.map((turn, i) => (
              <Turn
                key={turn.id}
                turn={turn}
                provider={provider}
                last={i === turns.length - 1}
                busy={busy}
              />
            ))}
          </ol>
        )}
      </div>
      {agreed ? (
        <Composer provider={provider} hasWorkspace={hasWorkspace} />
      ) : (
        <CloudAgreement provider={provider} />
      )}
    </>
  );
}

function SuggestionButton({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[13px] text-foreground hover:bg-accent [&_svg]:size-3.5 [&_svg]:text-muted-foreground"
    >
      {children}
    </button>
  );
}

/** What a message included, in words. */
export function describeContext(context: ContextReport): string | null {
  const parts: string[] = [];
  if (context.selection) {
    parts.push(context.selection_complete ? "the selection" : "part of the selection");
  }
  if (context.page) {
    parts.push(
      context.page_complete ? `“${context.page.title}”` : `part of “${context.page.title}”`,
    );
  }
  if (context.passages > 0) {
    parts.push(`${context.passages} passage${context.passages === 1 ? "" : "s"} from your notes`);
  }
  const sentences: string[] = [];
  if (parts.length > 0) {
    const list =
      parts.length === 1
        ? parts[0]
        : `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
    sentences.push(`Included ${list}.`);
  }
  if (context.searched && context.passages === 0) {
    sentences.push(parts.length > 0 ? "No other notes matched." : "No notes matched.");
  }
  if (context.left_out_turns > 0) {
    sentences.push(
      `${context.left_out_turns} earlier message${context.left_out_turns === 1 ? " was" : "s were"} left out to fit.`,
    );
  }
  return sentences.length > 0 ? sentences.join(" ") : null;
}

function Turn({
  turn,
  provider,
  last,
  busy,
}: {
  turn: ChatTurn;
  provider: ProviderConfig;
  last: boolean;
  busy: boolean;
}) {
  const retry = useAiChatStore((s) => s.retry);
  if (turn.role === "user") {
    const included = turn.context ? describeContext(turn.context) : null;
    return (
      <li className="flex flex-col items-end">
        <div className="selectable max-w-[90%] rounded-lg bg-muted px-3 py-2 text-[13.5px] leading-[1.6] whitespace-pre-wrap break-words">
          {turn.content}
        </div>
        {turn.sentTo && (
          <span className="mt-1 text-right text-[11px] leading-4 text-faint-foreground">
            {turn.sentTo}
            {included && (
              <>
                <br />
                {included}
              </>
            )}
          </span>
        )}
      </li>
    );
  }
  const canRetry = last && !busy && (turn.status === "failed" || turn.status === "stopped");
  return (
    <li aria-busy={turn.status === "streaming"}>
      {turn.activity.length > 0 && (
        <ul aria-label="Lookups" className="mb-1.5 flex flex-col gap-0.5">
          {turn.activity.map((step, i) => (
            <li
              // biome-ignore lint/suspicious/noArrayIndexKey: steps only grow
              key={i}
              className={cn(
                "flex items-center gap-1.5 text-[12px] text-muted-foreground",
                step.failed && "text-destructive",
              )}
            >
              {step.failed ? (
                <TriangleAlert className="size-3 shrink-0" aria-hidden />
              ) : (
                <Search className="size-3 shrink-0" aria-hidden />
              )}
              {step.label}
            </li>
          ))}
        </ul>
      )}
      {turn.content ? (
        <ChatMarkdown text={turn.content} sources={turn.sources} onOpenSource={openSource} />
      ) : (
        turn.status === "streaming" && (
          <p className="text-[13px] text-muted-foreground">Waiting for {provider.model}…</p>
        )
      )}
      {turn.status !== "streaming" && turn.sources.length > 0 && <Sources turn={turn} />}
      {turn.status === "stopped" && (
        <p className="mt-1 text-[12px] text-muted-foreground">Stopped.</p>
      )}
      {turn.status === "truncated" && (
        <p className="mt-1 text-[12px] text-muted-foreground">
          The answer was cut off at the length limit.
        </p>
      )}
      {turn.status === "failed" && turn.error && (
        <div role="alert" className="mt-2 rounded-md border px-3 py-2 text-[12.5px] leading-5">
          <p className="text-destructive">{turn.error.message}</p>
          {turn.error.retry_after_secs !== null && (
            <p className="mt-0.5 text-muted-foreground">
              Try again in {turn.error.retry_after_secs} s.
            </p>
          )}
        </div>
      )}
      {canRetry && (
        <button
          type="button"
          onClick={() => retry(provider)}
          className="mt-2 inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[12.5px] text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          <RotateCcw className="size-3.5" aria-hidden /> Retry
        </button>
      )}
    </li>
  );
}

/** The sources an answer cites, each opening its page; uncited ones on request. */
function Sources({ turn }: { turn: ChatTurn }) {
  const { cited, unverified } = checkCitations(turn.content, turn.sources);
  const uncited = turn.sources.filter((s) => !cited.includes(s));
  return (
    <div className="mt-2 text-[12px]">
      {cited.length > 0 && <SourceList label="Sources" sources={cited} />}
      {uncited.length > 0 && (
        <details className="mt-1 text-muted-foreground">
          <summary className="cursor-default rounded px-1 hover:text-foreground">
            {cited.length > 0
              ? `${uncited.length} more provided, not cited`
              : `${uncited.length} source${uncited.length === 1 ? "" : "s"} provided, none cited`}
          </summary>
          <SourceList label="Not cited" sources={uncited} />
        </details>
      )}
      {unverified.length > 0 && (
        <p className="mt-1 flex items-start gap-1 px-1 text-muted-foreground">
          <TriangleAlert className="mt-0.5 size-3 shrink-0" aria-hidden />
          <span>
            Unverified: {unverified.join(", ")} {unverified.length === 1 ? "isn't" : "aren't"} among
            the sources Forma provided.
          </span>
        </p>
      )}
    </div>
  );
}

function SourceList({ label, sources }: { label: string; sources: Source[] }) {
  return (
    <ul aria-label={label} className="flex flex-col">
      {sources.map((source) => (
        <li key={source.key}>
          <button
            type="button"
            onClick={() => openSource(source)}
            title={source.excerpt}
            className="flex w-full min-w-0 items-baseline gap-1.5 rounded px-1 py-0.5 text-left hover:bg-accent"
          >
            <span className="shrink-0 rounded bg-muted px-1 text-[10.5px] font-medium text-muted-foreground">
              {source.key}
            </span>
            <span className="truncate text-foreground">
              {source.title}
              {source.section.length > 0 && (
                <span className="text-muted-foreground"> › {source.section.join(" › ")}</span>
              )}
              {source.kind === "selection" && (
                <span className="text-muted-foreground"> (selection)</span>
              )}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

function Destination({ provider, notes }: { provider: ProviderConfig; notes: boolean }) {
  const local = isLocalUrl(provider.base_url);
  const Icon = local ? Laptop : Cloud;
  return (
    <p className="mt-1.5 flex items-start gap-1.5 px-1 text-[11.5px] leading-4 text-faint-foreground">
      <Icon className="mt-0.5 size-3 shrink-0" aria-hidden />
      <span>
        {destinationLabel(provider)} · {provider.model} ·{" "}
        {notes ? "with passages from your notes" : "no notes included"}
      </span>
    </p>
  );
}

function Chip({
  icon,
  label,
  onRemove,
  removeLabel,
}: {
  icon: ReactNode;
  label: string;
  onRemove: () => void;
  removeLabel: string;
}) {
  return (
    <span className="inline-flex max-w-full items-center gap-1 rounded-md bg-muted py-0.5 pr-0.5 pl-1.5 text-[12px] text-foreground [&_svg]:size-3 [&_svg]:shrink-0">
      {icon}
      <span className="truncate">{label}</span>
      <button
        type="button"
        aria-label={removeLabel}
        title={removeLabel}
        onClick={onRemove}
        className="rounded-sm p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
      >
        <X aria-hidden />
      </button>
    </span>
  );
}

function Composer({ provider, hasWorkspace }: { provider: ProviderConfig; hasWorkspace: boolean }) {
  const text = useAiChatStore((s) => s.draft);
  const busy = useAiChatStore((s) => s.requestId !== null);
  const page = useAiChatStore((s) => s.page);
  const selection = useAiChatStore((s) => s.selection);
  const searchNotes = useAiChatStore((s) => s.searchNotes);
  const focusRequest = useAiChatStore((s) => s.focusRequest);
  const { send, stop, setDraft, attachPage, attachSelection, setSearchNotes } =
    useAiChatStore.getState();
  const openPage = useOpenPage();
  const input = useRef<HTMLTextAreaElement>(null);
  const visible = useSettingsStore((s) => s.settings.layout.ai_panel.visible);

  // The panel stays mounted while hidden; focus the message box when it
  // opens, and when an action asks for it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: focusRequest is the trigger
  useEffect(() => {
    if (visible) input.current?.focus();
  }, [visible, focusRequest]);

  function submit() {
    if (busy || !text.trim()) return;
    void send(text, provider);
    setDraft("");
  }

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    // Enter sends; Shift+Enter is a new line. Not while composing CJK input.
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      submit();
    }
  }

  const notes = hasWorkspace && (searchNotes || page !== null || selection !== null);
  const canAddPage = hasWorkspace && openPage !== null && page?.id !== openPage.id;
  return (
    <div className="shrink-0 px-3 pb-3">
      <div className="rounded-lg px-2.5 py-2 shadow-popover">
        {hasWorkspace && (
          <div className="mb-1.5 flex flex-wrap items-center gap-1">
            {selection && (
              <Chip
                icon={<TextQuote aria-hidden />}
                label={`“${selection.text.replace(/\s+/g, " ")}”`}
                onRemove={() => attachSelection(null)}
                removeLabel="Remove the selection"
              />
            )}
            {page && (
              <Chip
                icon={<FileText aria-hidden />}
                label={page.title}
                onRemove={() => attachPage(null)}
                removeLabel={`Remove “${page.title}”`}
              />
            )}
            {canAddPage && openPage && (
              <button
                type="button"
                onClick={() => attachPage(openPage)}
                className="inline-flex max-w-full items-center gap-1 rounded-md px-1.5 py-0.5 text-[12px] text-muted-foreground hover:bg-accent hover:text-foreground [&_svg]:size-3"
              >
                <FileText aria-hidden className="shrink-0" />
                <span className="truncate">Add “{openPage.title}”</span>
              </button>
            )}
            <button
              type="button"
              aria-pressed={searchNotes}
              onClick={() => setSearchNotes(!searchNotes)}
              title={
                searchNotes
                  ? "Each message searches your notes for passages to include"
                  : "Messages don't search your notes"
              }
              className={cn(
                "ml-auto inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[12px] [&_svg]:size-3",
                searchNotes
                  ? "text-foreground hover:bg-accent"
                  : "text-faint-foreground line-through hover:bg-accent",
              )}
            >
              <Search aria-hidden /> Search notes
            </button>
          </div>
        )}
        <div className="flex items-end gap-1.5">
          <textarea
            ref={input}
            aria-label="Message"
            rows={1}
            value={text}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={`Message ${provider.model}`}
            className="max-h-40 min-h-6 flex-1 resize-none bg-transparent text-[13.5px] leading-6 outline-none [field-sizing:content] placeholder:text-faint-foreground"
          />
          {busy ? (
            <ComposerButton label="Stop" onClick={stop}>
              <Square className="size-3 fill-current" aria-hidden />
            </ComposerButton>
          ) : (
            <ComposerButton label="Send" onClick={submit} disabled={!text.trim()}>
              <ArrowUp className="size-4" aria-hidden />
            </ComposerButton>
          )}
        </div>
      </div>
      <Destination provider={provider} notes={notes} />
    </div>
  );
}

function ComposerButton({
  label,
  onClick,
  disabled,
  children,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "inline-flex size-7 shrink-0 items-center justify-center rounded-md bg-foreground text-background",
        "disabled:bg-muted disabled:text-faint-foreground",
      )}
    >
      {children}
    </button>
  );
}

/** The first use of a cloud provider needs the user's explicit agreement (ARCHITECTURE §9.3). */
function CloudAgreement({ provider }: { provider: ProviderConfig }) {
  const [error, setError] = useState<string | null>(null);
  async function agree() {
    try {
      useSettingsStore.getState().applyAi(await ipc.aiAcknowledgeCloud(provider.id));
    } catch (e) {
      setError((e as Error).message);
    }
  }
  return (
    <div className="shrink-0 px-3 pb-3">
      <div className="rounded-lg px-3 py-3 shadow-popover">
        <p className="flex items-center gap-1.5 text-[13px] font-medium">
          <Cloud className="size-3.5" aria-hidden /> Messages leave this Mac
        </p>
        <p className="mt-1 text-[12.5px] leading-5 text-muted-foreground">
          What you write here, and the parts of your notes needed to answer it, go to{" "}
          {provider.name} at {hostOf(provider.base_url)}, under its terms. Turn off “Search notes”
          and attach nothing to send only what you type.
        </p>
        {error && <p className="mt-1 text-[12.5px] text-destructive">{error}</p>}
        <div className="mt-2.5 flex gap-2">
          <button
            type="button"
            onClick={() => void agree()}
            className="rounded-md bg-primary px-3 py-1.5 text-[13px] text-primary-foreground"
          >
            Continue with {provider.name}
          </button>
          <button
            type="button"
            onClick={() => useUiStore.getState().openSettings("ai")}
            className="rounded-md px-3 py-1.5 text-[13px] text-muted-foreground hover:bg-accent"
          >
            Settings
          </button>
        </div>
      </div>
    </div>
  );
}
