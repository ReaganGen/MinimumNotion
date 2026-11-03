// The AI actions offered outside the panel: on a page (Ask about this page,
// Summarize page) and on selected text (Ask about selection, Explain
// selection). Each opens the panel with the page or text attached; the
// prompt actions also send, when a provider is ready.
import { useRevealStore } from "@/features/editor/reveal";
import { useSettingsStore } from "@/features/settings/settingsStore";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { Source } from "@/lib/ipc/bindings/Source";
import { type PageAttachment, type SelectionAttachment, useAiChatStore } from "./aiStore";
import { readiness } from "./providers";

export const SUMMARIZE_PROMPT = "Summarize this page.";
export const EXPLAIN_PROMPT = "Explain the selected text.";

function openPanel() {
  useSettingsStore.getState().setPanelVisible("ai_panel", true);
}

/** Sends `prompt` now if a provider is ready; otherwise leaves it in the message box. */
function sendOrDraft(prompt: string) {
  const chat = useAiChatStore.getState();
  const ready = readiness(useSettingsStore.getState().settings.ai);
  if (ready.state === "ready" && chat.requestId === null) {
    void chat.send(prompt, ready.provider);
  } else {
    chat.setDraft(prompt);
  }
}

export function askAboutPage(page: PageAttachment) {
  openPanel();
  const chat = useAiChatStore.getState();
  chat.attachPage(page);
  chat.requestFocus();
}

export function summarizePage(page: PageAttachment) {
  openPanel();
  useAiChatStore.getState().attachPage(page);
  sendOrDraft(SUMMARIZE_PROMPT);
}

export function askAboutSelection(selection: SelectionAttachment) {
  openPanel();
  const chat = useAiChatStore.getState();
  chat.attachSelection(selection);
  chat.requestFocus();
}

export function explainSelection(selection: SelectionAttachment) {
  openPanel();
  useAiChatStore.getState().attachSelection(selection);
  sendOrDraft(EXPLAIN_PROMPT);
}

/** Opens a cited source's page and shows the cited text. */
export function openSource(source: Source) {
  useRevealStore.getState().reveal(source.page_id, source.excerpt);
  useWorkspaceStore.getState().openPage(source.page_id);
}
