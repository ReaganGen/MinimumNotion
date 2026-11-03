// The AI chat. Transcripts live in memory for this session only (DATA_MODEL §11.3).
import { create } from "zustand";
import type { AiError } from "@/lib/ipc/bindings/AiError";
import type { ChatStreamEvent } from "@/lib/ipc/bindings/ChatStreamEvent";
import type { Message } from "@/lib/ipc/bindings/Message";
import type { ProviderConfig } from "@/lib/ipc/bindings/ProviderConfig";
import { ipc } from "@/lib/ipc/client";
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
}

interface AiChatState {
  turns: ChatTurn[];
  /** The running request, or null when idle. */
  requestId: string | null;
  send: (text: string, provider: ProviderConfig) => Promise<void>;
  stop: () => void;
  /** Sends the last question again, replacing the failed or stopped answer. */
  retry: (provider: ProviderConfig) => void;
  clear: () => void;
}

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
  function updateAnswer(id: string, change: (turn: ChatTurn) => ChatTurn) {
    set({ turns: get().turns.map((t) => (t.id === id ? change(t) : t)) });
  }

  return {
    turns: [],
    requestId: null,

    send: async (text, provider) => {
      const content = text.trim();
      if (!content || get().requestId !== null) return;
      const question: ChatTurn = {
        id: turnId(),
        role: "user",
        content,
        status: "done",
        sentTo: `${destinationLabel(provider)} · ${provider.model}`,
      };
      const answer: ChatTurn = {
        id: turnId(),
        role: "assistant",
        content: "",
        status: "streaming",
      };
      const messages = history([...get().turns, question]);
      const requestId = newRequestId();
      set({ turns: [...get().turns, question, answer], requestId });

      const onEvent = (event: ChatStreamEvent) => {
        switch (event.type) {
          case "text":
            updateAnswer(answer.id, (t) => ({ ...t, content: t.content + event.text }));
            return;
          case "done":
            updateAnswer(answer.id, (t) => ({
              ...t,
              status: event.stop_reason === "max_tokens" ? "truncated" : "done",
            }));
            return;
          case "failed":
            updateAnswer(answer.id, (t) => ({
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
        await ipc.aiChat(requestId, messages, onEvent);
      } catch (error) {
        updateAnswer(answer.id, (t) => ({
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
        updateAnswer(answer.id, (t) =>
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
      set({ turns: [], requestId: null });
    },
  };
});

export function resetAiChatStoreForTests() {
  useAiChatStore.setState({ turns: [], requestId: null });
}
