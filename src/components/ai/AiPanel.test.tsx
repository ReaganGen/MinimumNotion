import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { resetAiChatStoreForTests } from "@/features/ai/aiStore";
import { defaultSettings } from "@/features/settings/defaults";
import { resetSettingsStoreForTests, useSettingsStore } from "@/features/settings/settingsStore";
import { useUiStore } from "@/features/ui/uiStore";
import type { AiSettings } from "@/lib/ipc/bindings/AiSettings";
import type { ChatStreamEvent } from "@/lib/ipc/bindings/ChatStreamEvent";
import type { ProviderConfig } from "@/lib/ipc/bindings/ProviderConfig";
import { ipc } from "@/lib/ipc/client";
import { AiPanel } from "./AiPanel";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    aiChat: vi.fn(),
    aiCancel: vi.fn(),
    aiAcknowledgeCloud: vi.fn(),
    openExternal: vi.fn(),
    saveSettings: vi.fn(),
  },
}));

const OPTIONS = {
  temperature: null,
  max_output_tokens: null,
  context_window_override: null,
  tool_calling_override: null,
};

function provider(overrides: Partial<ProviderConfig> = {}): ProviderConfig {
  return {
    id: "p1",
    kind: "ollama",
    name: "Ollama",
    base_url: "http://localhost:11434",
    model: "llama3.2",
    has_credential: false,
    options: OPTIONS,
    ...overrides,
  };
}

function setAi(ai: Partial<AiSettings>) {
  const settings = defaultSettings();
  useSettingsStore.setState({
    settings: {
      ...settings,
      ai: { ...settings.ai, enabled: true, active_provider: "p1", ...ai },
    },
  });
}

/** Captures the stream handler of the latest chat so a test can play events into it. */
function captureChat() {
  const chat: { send?: (e: ChatStreamEvent) => void; finish?: () => void } = {};
  vi.mocked(ipc.aiChat).mockImplementation(
    (_id, _messages, onEvent) =>
      new Promise<void>((resolve) => {
        chat.send = (e) => act(() => onEvent(e));
        chat.finish = () => resolve();
      }),
  );
  return chat;
}

function renderPanel() {
  render(
    <TooltipProvider>
      <AiPanel />
    </TooltipProvider>,
  );
}

beforeEach(() => {
  resetSettingsStoreForTests();
  resetAiChatStoreForTests();
  useUiStore.setState({ dialog: null, settingsSection: "appearance" });
  for (const fn of Object.values(ipc)) vi.mocked(fn).mockReset();
  vi.mocked(ipc.aiCancel).mockResolvedValue(undefined);
});

describe("AiPanel", () => {
  it("says AI is off and opens its settings", async () => {
    const user = userEvent.setup();
    renderPanel();
    expect(screen.getByText("AI is off")).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Message" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Turn on in Settings" }));
    expect(useUiStore.getState()).toMatchObject({ dialog: "settings", settingsSection: "ai" });
  });

  it("asks for what's missing before chatting", () => {
    setAi({ providers: [provider({ model: "" })] });
    renderPanel();
    expect(screen.getByText("Choose a model")).toBeInTheDocument();
    act(() =>
      setAi({
        providers: [
          provider({ kind: "anthropic", name: "Anthropic", base_url: "https://api.anthropic.com" }),
        ],
      }),
    );
    expect(screen.getByText("Add an API key")).toBeInTheDocument();
  });

  it("needs agreement before the first message to a cloud provider", async () => {
    const user = userEvent.setup();
    const anthropic = provider({
      kind: "anthropic",
      name: "Anthropic",
      base_url: "https://api.anthropic.com",
      model: "claude-x",
      has_credential: true,
    });
    setAi({ providers: [anthropic] });
    const agreed = defaultSettings();
    agreed.ai = {
      enabled: true,
      active_provider: "p1",
      providers: [anthropic],
      cloud_acknowledged: { p1: "2026-11-03T10:00:00Z" },
    };
    vi.mocked(ipc.aiAcknowledgeCloud).mockResolvedValue(agreed);
    renderPanel();

    expect(screen.getByText("Messages leave this Mac")).toBeInTheDocument();
    expect(screen.getByText(/goes to Anthropic at api\.anthropic\.com/)).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Message" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Continue with Anthropic" }));
    expect(ipc.aiAcknowledgeCloud).toHaveBeenCalledWith("p1");
    expect(await screen.findByRole("textbox", { name: "Message" })).toBeInTheDocument();
    expect(
      screen.getByText("Sent to Anthropic (api.anthropic.com) · claude-x · no notes included"),
    ).toBeInTheDocument();
  });

  it("streams an answer and says where the message went", async () => {
    const user = userEvent.setup();
    setAi({ providers: [provider()] });
    const chat = captureChat();
    renderPanel();
    expect(screen.getByText(/Stays on this Mac · llama3.2/)).toBeInTheDocument();

    const box = screen.getByRole("textbox", { name: "Message" });
    await user.type(box, "Line one{Shift>}{Enter}{/Shift}line two");
    expect(ipc.aiChat).not.toHaveBeenCalled();
    await user.type(box, "{Enter}");
    expect(ipc.aiChat).toHaveBeenCalledWith(
      expect.any(String),
      [{ role: "user", content: "Line one\nline two" }],
      expect.any(Function),
    );
    expect(box).toHaveValue("");
    const conversation = screen.getByRole("list", { name: "Conversation" });
    expect(within(conversation).getByText("Stays on this Mac · llama3.2")).toBeInTheDocument();
    expect(within(conversation).getByText("Waiting for llama3.2…")).toBeInTheDocument();

    chat.send?.({ type: "text", text: "Hello **wor" });
    chat.send?.({ type: "text", text: "ld**" });
    chat.send?.({ type: "done", stop_reason: "end_turn" });
    await act(async () => chat.finish?.());
    expect(conversation.querySelector("strong")).toHaveTextContent("world");
    expect(screen.getByRole("button", { name: "Send" })).toBeInTheDocument();
  });

  it("stops an answer and offers to retry it", async () => {
    const user = userEvent.setup();
    setAi({ providers: [provider()] });
    const chat = captureChat();
    renderPanel();
    await user.type(screen.getByRole("textbox", { name: "Message" }), "Tell me a story{Enter}");
    chat.send?.({ type: "text", text: "Once upon" });

    await user.click(screen.getByRole("button", { name: "Stop" }));
    const [requestId] = vi.mocked(ipc.aiChat).mock.calls[0] ?? [];
    expect(ipc.aiCancel).toHaveBeenCalledWith(requestId);
    chat.send?.({
      type: "failed",
      error: { kind: "cancelled", message: "Stopped.", status: null, retry_after_secs: null },
    });
    await act(async () => chat.finish?.());
    expect(screen.getByText("Once upon")).toBeInTheDocument();
    expect(screen.getByText("Stopped.")).toBeInTheDocument();

    const again = captureChat();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(ipc.aiChat).toHaveBeenLastCalledWith(
      expect.any(String),
      [{ role: "user", content: "Tell me a story" }],
      expect.any(Function),
    );
    again.send?.({ type: "done", stop_reason: "end_turn" });
  });

  it("shows provider failures clearly", async () => {
    const user = userEvent.setup();
    setAi({ providers: [provider()] });
    const chat = captureChat();
    renderPanel();
    await user.type(screen.getByRole("textbox", { name: "Message" }), "Hi{Enter}");
    chat.send?.({
      type: "failed",
      error: {
        kind: "rate_limited",
        message: "Ollama is limiting requests.",
        status: 429,
        retry_after_secs: 20,
      },
    });
    await act(async () => chat.finish?.());
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Ollama is limiting requests.");
    expect(alert).toHaveTextContent("Try again in 20 s.");
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  it("keeps a failed answer out of the next request, and clears", async () => {
    const user = userEvent.setup();
    setAi({ providers: [provider()] });
    let chat = captureChat();
    renderPanel();
    const box = screen.getByRole("textbox", { name: "Message" });
    await user.type(box, "First{Enter}");
    chat.send?.({ type: "text", text: "Answer one" });
    chat.send?.({ type: "done", stop_reason: "end_turn" });
    await act(async () => chat.finish?.());

    chat = captureChat();
    await user.type(box, "Second{Enter}");
    expect(ipc.aiChat).toHaveBeenLastCalledWith(
      expect.any(String),
      [
        { role: "user", content: "First" },
        { role: "assistant", content: "Answer one" },
        { role: "user", content: "Second" },
      ],
      expect.any(Function),
    );
    chat.send?.({ type: "done", stop_reason: "end_turn" });
    await act(async () => chat.finish?.());

    await user.click(screen.getByRole("button", { name: "Clear the conversation" }));
    expect(screen.queryByRole("list", { name: "Conversation" })).toBeNull();
  });
});
