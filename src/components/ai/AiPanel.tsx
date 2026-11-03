import { ArrowUp, Cloud, Laptop, RotateCcw, Sparkles, Square, Trash2, X } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { IconButton } from "@/components/layout/IconButton";
import { type ChatTurn, useAiChatStore } from "@/features/ai/aiStore";
import { destinationLabel, hostOf, isLocalUrl, readiness } from "@/features/ai/providers";
import { runCommand, SHORTCUTS } from "@/features/commands/commands";
import { useSettingsStore } from "@/features/settings/settingsStore";
import { useUiStore } from "@/features/ui/uiStore";
import type { ProviderConfig } from "@/lib/ipc/bindings/ProviderConfig";
import { ipc } from "@/lib/ipc/client";
import { cn } from "@/lib/utils";
import { ChatMarkdown } from "./ChatMarkdown";

/** The AI assistant panel. Plain chat with the active provider; it sees no notes yet. */
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
          body="Forma works fully without it. Turn it on to chat with OpenAI, Anthropic, or a local model through Ollama or LM Studio."
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

function Conversation({ provider, agreed }: { provider: ProviderConfig; agreed: boolean }) {
  const turns = useAiChatStore((s) => s.turns);
  const busy = useAiChatStore((s) => s.requestId !== null);
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
          <p className="mt-6 text-[13px] leading-5 text-muted-foreground">
            Ask anything. The assistant can't see your notes yet: it only knows what you write here.
          </p>
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
      {agreed ? <Composer provider={provider} /> : <CloudAgreement provider={provider} />}
    </>
  );
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
    return (
      <li className="flex flex-col items-end">
        <div className="selectable max-w-[90%] rounded-lg bg-muted px-3 py-2 text-[13.5px] leading-[1.6] whitespace-pre-wrap break-words">
          {turn.content}
        </div>
        {turn.sentTo && (
          <span className="mt-1 text-[11px] text-faint-foreground">{turn.sentTo}</span>
        )}
      </li>
    );
  }
  const canRetry = last && !busy && (turn.status === "failed" || turn.status === "stopped");
  return (
    <li aria-busy={turn.status === "streaming"}>
      {turn.content ? (
        <ChatMarkdown text={turn.content} />
      ) : (
        turn.status === "streaming" && (
          <p className="text-[13px] text-muted-foreground">Waiting for {provider.model}…</p>
        )
      )}
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

function Destination({ provider }: { provider: ProviderConfig }) {
  const local = isLocalUrl(provider.base_url);
  const Icon = local ? Laptop : Cloud;
  return (
    <p className="mt-1.5 flex items-start gap-1.5 px-1 text-[11.5px] leading-4 text-faint-foreground">
      <Icon className="mt-0.5 size-3 shrink-0" aria-hidden />
      <span>
        {destinationLabel(provider)} · {provider.model} · no notes included
      </span>
    </p>
  );
}

function Composer({ provider }: { provider: ProviderConfig }) {
  const [text, setText] = useState("");
  const busy = useAiChatStore((s) => s.requestId !== null);
  const { send, stop } = useAiChatStore.getState();
  const input = useRef<HTMLTextAreaElement>(null);
  const visible = useSettingsStore((s) => s.settings.layout.ai_panel.visible);

  // The panel stays mounted while hidden; focus the message box when it opens.
  useEffect(() => {
    if (visible) input.current?.focus();
  }, [visible]);

  function submit() {
    if (busy || !text.trim()) return;
    void send(text, provider);
    setText("");
  }

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    // Enter sends; Shift+Enter is a new line. Not while composing CJK input.
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      submit();
    }
  }

  return (
    <div className="shrink-0 px-3 pb-3">
      <div className="flex items-end gap-1.5 rounded-lg px-2.5 py-2 shadow-popover">
        <textarea
          ref={input}
          aria-label="Message"
          rows={1}
          value={text}
          onChange={(e) => setText(e.target.value)}
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
      <Destination provider={provider} />
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
          What you write here goes to {provider.name} at {hostOf(provider.base_url)}, under its
          terms. Forma sends only what you type in this panel, never your notes.
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
