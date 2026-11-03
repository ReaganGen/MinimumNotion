// The AI chat. Transcripts live in memory for this session only (DATA_MODEL §11.3).
// What a message may include from the notes is chosen here (an attached page,
// a selection, searching the notes); Rust reads the notes and reports back
// exactly what it sent.
import { create } from "zustand";
import { cursorBlockText } from "@/features/editor/activeEditor";
import type { AiError } from "@/lib/ipc/bindings/AiError";
import type { AskContext } from "@/lib/ipc/bindings/AskContext";
import type { ChatStreamEvent } from "@/lib/ipc/bindings/ChatStreamEvent";
import type { ContextReport } from "@/lib/ipc/bindings/ContextReport";
import type { Message } from "@/lib/ipc/bindings/Message";
import type { ProviderConfig } from "@/lib/ipc/bindings/ProviderConfig";
import type { Source } from "@/lib/ipc/bindings/Source";
import type { ToolActivity } from "@/lib/ipc/bindings/ToolActivity";
import { ipc } from "@/lib/ipc/client";
import { nextSourceNumber } from "./citations";
import { destinationLabel } from "./providers";

export type TurnStatus = "streaming" | "done" | "truncated" | "stopped" | "failed";

export interface ChatTurn {
  id: string;
  role: "user" | "assistant";
  content: string;
  status: TurnStatus;
  error?: AiError;
  /** For user turns: where the message went and to which model, recorded at send time. */
  sentTo?: string;
  /** For user turns: what Rust included with the message. */
  context?: ContextReport;
  /** For assistant turns: the notes given to the model, under the keys it cites. */
  sources: Source[];
  /** For assistant turns: what the model looked up. */
  activity: ToolActivity[];
}

export interface PageAttachment {
  id: string;
  title: string;
}

export interface SelectionAttachment {
  pageId: string;
  title: string;
  text: string;
}

interface AiChatState {
  turns: ChatTurn[];
  /** The running request, or null when idle. */
  requestId: string | null;
  page: PageAttachment | null;
  selection: SelectionAttachment | null;
  /** Search the notes for each message. */
  searchNotes: boolean;
  /** The composer's text, so actions can fill it in. */
  draft: string;
  /** Bumped when an action wants the message box focused. */
  focusRequest: number;
  setDraft: (text: string) => void;
  attachPage: (page: PageAttachment | null) => void;
  attachSelection: (selection: SelectionAttachment | null) => void;
  setSearchNotes: (on: boolean) => void;
  requestFocus: () => void;
  send: (text: string, provider: ProviderConfig) => Promise<void>;
  stop: () => void;
  /** Sends the last question again, replacing the failed or stopped answer. */
  retry: (provider: ProviderConfig) => void;
  clear: () => void;
}

/** As `MAX_SELECTION_CHARS` in crates/forma-ai/src/knowledge.rs. */
const MAX_SELECTION_CHARS = 20_000;

let nextTurn = 0;
const turnId = () => `turn-${++nextTurn}`;

/** A random request ID. `getRandomValues` works in any context, unlike `randomUUID`. */
function newRequestId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/** The conversation as the model sees it: failed answers and empty ones are left out. */
function history(turns: ChatTurn[]): Message[] {
  return turns
    .filter((t) => t.status !== "failed" && t.content.trim() !== "")
    .map((t) => ({ role: t.role, content: t.content }));
}

export const useAiChatStore = create<AiChatState>()((set, get) => {
  function update(id: string, change: (turn: ChatTurn) => ChatTurn) {
    set({ turns: get().turns.map((t) => (t.id === id ? change(t) : t)) });
  }

  return {
    turns: [],
    requestId: null,
    page: null,
    selection: null,
    searchNotes: true,
    draft: "",
    focusRequest: 0,
    setDraft: (draft) => set({ draft }),
    attachPage: (page) => set({ page }),
    // Rust refuses a longer one; this keeps the start instead.
    attachSelection: (selection) =>
      set({
        selection: selection && {
          ...selection,
          text: selection.text.slice(0, MAX_SELECTION_CHARS),
        },
      }),
    setSearchNotes: (searchNotes) => set({ searchNotes }),
    requestFocus: () => set({ focusRequest: get().focusRequest + 1 }),

    send: async (text, provider) => {
      const content = text.trim();
      const { turns, requestId: running, page, selection, searchNotes } = get();
      if (!content || running !== null) return;
      const question: ChatTurn = {
        id: turnId(),
        role: "user",
        content,
        status: "done",
        sentTo: `${destinationLabel(provider)} · ${provider.model}`,
        sources: [],
        activity: [],
      };
      const answer: ChatTurn = {
        id: turnId(),
        role: "assistant",
        content: "",
        status: "streaming",
        sources: [],
        activity: [],
      };
      const context: AskContext = {
        page_id: page?.id ?? null,
        selection: selection ? { page_id: selection.pageId, text: selection.text } : null,
        focus: page ? cursorBlockText(page.id) : null,
        search_notes: searchNotes,
        first_source: nextSourceNumber(turns.flatMap((t) => t.sources)),
      };
      const messages = history([...turns, question]);
      const requestId = newRequestId();
      set({ turns: [...turns, question, answer], requestId });

      const onEvent = (event: ChatStreamEvent) => {
        switch (event.type) {
          case "context":
            update(question.id, (t) => ({ ...t, context: event.context }));
            return;
          case "sources":
            update(answer.id, (t) => ({ ...t, sources: [...t.sources, ...event.sources] }));
            return;
          case "tool":
            update(answer.id, (t) => ({ ...t, activity: [...t.activity, event.activity] }));
            return;
          case "text":
            update(answer.id, (t) => ({ ...t, content: t.content + event.text }));
            return;
          case "done":
            update(answer.id, (t) => ({
              ...t,
              status: event.stop_reason === "max_tokens" ? "truncated" : "done",
            }));
            return;
          case "failed":
            update(answer.id, (t) => ({
              ...t,
              status: event.error.kind === "cancelled" ? "stopped" : "failed",
              error: event.error,
            }));
            return;
          case "usage":
            return;
        }
      };
      try {
        await ipc.aiChat(requestId, { messages, context }, onEvent);
      } catch (error) {
        update(answer.id, (t) => ({
          ...t,
          status: "failed",
          error: {
            kind: "provider",
            message: (error as Error).message,
            status: null,
            retry_after_secs: null,
          },
        }));
      } finally {
        // An answer that never said how it ended didn't finish.
        update(answer.id, (t) =>
          t.status === "streaming"
            ? {
                ...t,
                status: "failed",
                error: {
                  kind: "interrupted",
                  message: "The answer stopped unexpectedly.",
                  status: null,
                  retry_after_secs: null,
                },
              }
            : t,
        );
        if (get().requestId === requestId) set({ requestId: null });
      }
    },

    stop: () => {
      const { requestId } = get();
      if (requestId) void ipc.aiCancel(requestId).catch(() => {});
    },

    retry: (provider) => {
      const turns = get().turns;
      const lastQuestion = turns.findLastIndex((t) => t.role === "user");
      const question = turns[lastQuestion];
      if (!question || get().requestId !== null) return;
      set({ turns: turns.slice(0, lastQuestion) });
      void get().send(question.content, provider);
    },

    clear: () => {
      get().stop();
      set({ turns: [], requestId: null, page: null, selection: null });
    },
  };
});

export function resetAiChatStoreForTests() {
  useAiChatStore.setState({
    turns: [],
    requestId: null,
    page: null,
    selection: null,
    searchNotes: true,
    draft: "",
    focusRequest: 0,
  });
}
