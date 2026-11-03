import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "@/features/settings/defaults";
import { resetSettingsStoreForTests, useSettingsStore } from "@/features/settings/settingsStore";
import type { AiSettings } from "@/lib/ipc/bindings/AiSettings";
import type { AppSettings } from "@/lib/ipc/bindings/AppSettings";
import type { ProviderConfig } from "@/lib/ipc/bindings/ProviderConfig";
import { IpcError, ipc } from "@/lib/ipc/client";
import { AiSection } from "./AiSection";

vi.mock("@/lib/ipc/client", async (original) => ({
  IpcError: (await original<typeof import("@/lib/ipc/client")>()).IpcError,
  ipc: {
    aiSetEnabled: vi.fn(),
    aiAddProvider: vi.fn(),
    aiUpdateProvider: vi.fn(),
    aiRemoveProvider: vi.fn(),
    aiSetActiveProvider: vi.fn(),
    aiSetApiKey: vi.fn(),
    aiRemoveApiKey: vi.fn(),
    aiListModels: vi.fn(),
    aiModelCapabilities: vi.fn(),
    aiTestConnection: vi.fn(),
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
    kind: "openai",
    name: "OpenAI",
    base_url: "https://api.openai.com/v1",
    model: "",
    has_credential: false,
    options: OPTIONS,
    ...overrides,
  };
}

function withAi(ai: Partial<AiSettings>): AppSettings {
  const settings = defaultSettings();
  return { ...settings, ai: { ...settings.ai, ...ai } };
}

function useAi(ai: Partial<AiSettings>) {
  useSettingsStore.setState({ settings: withAi(ai) });
}

beforeEach(() => {
  resetSettingsStoreForTests();
  for (const fn of Object.values(ipc)) vi.mocked(fn).mockReset();
  vi.mocked(ipc.aiModelCapabilities).mockResolvedValue({
    tool_calling: "unknown",
    context_window: null,
    max_output_tokens: null,
  });
});

describe("AiSection", () => {
  it("turns AI on, then adds a provider and opens it for editing", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.aiSetEnabled).mockResolvedValue(withAi({ enabled: true }));
    vi.mocked(ipc.aiAddProvider).mockResolvedValue(
      withAi({
        enabled: true,
        active_provider: "p1",
        providers: [
          provider({ kind: "ollama", name: "Ollama", base_url: "http://localhost:11434" }),
        ],
      }),
    );
    render(<AiSection />);
    const toggle = screen.getByRole("switch", { name: "Use AI" });
    expect(toggle).not.toBeChecked();
    expect(screen.queryByText("Providers")).toBeNull();

    await user.click(toggle);
    expect(ipc.aiSetEnabled).toHaveBeenCalledWith(true);
    expect(screen.getByRole("switch", { name: "Use AI" })).toBeChecked();

    await user.click(screen.getByRole("button", { name: "Add provider" }));
    await user.click(await screen.findByRole("menuitem", { name: /Ollama/ }));
    expect(ipc.aiAddProvider).toHaveBeenCalledWith("ollama");
    expect(await screen.findByLabelText("Name")).toHaveValue("Ollama");
    expect(screen.getByText("Stays on this Mac.")).toBeInTheDocument();
    expect(screen.getByText("Optional. Only if this server asks for one.")).toBeInTheDocument();
  });

  it("hands the key to the Keychain and never shows it again", async () => {
    const user = userEvent.setup();
    useAi({ enabled: true, active_provider: "p1", providers: [provider()] });
    vi.mocked(ipc.aiSetApiKey).mockResolvedValue(
      withAi({
        enabled: true,
        active_provider: "p1",
        providers: [provider({ has_credential: true })],
      }),
    );
    const { container } = render(<AiSection />);
    const field = screen.getByLabelText("API key");
    expect(field).toHaveAttribute("type", "password");
    await user.type(field, "sk-proj-SECRET123");
    await user.click(screen.getByRole("button", { name: "Save key" }));

    expect(ipc.aiSetApiKey).toHaveBeenCalledWith("p1", "sk-proj-SECRET123");
    expect(await screen.findByText("Stored in your Keychain.")).toBeInTheDocument();
    expect(container.innerHTML).not.toContain("SECRET123");
    expect(screen.getByRole("button", { name: "Replace" })).toBeInTheDocument();
  });

  it("loads the provider's models and saves the one chosen", async () => {
    const user = userEvent.setup();
    useAi({
      enabled: true,
      active_provider: "p1",
      providers: [provider({ has_credential: true })],
    });
    vi.mocked(ipc.aiListModels).mockResolvedValue([
      { id: "gpt-6-astra", name: null, details: null },
      { id: "gpt-5.5", name: null, details: null },
    ]);
    vi.mocked(ipc.aiUpdateProvider).mockResolvedValue(
      withAi({
        enabled: true,
        active_provider: "p1",
        providers: [provider({ has_credential: true, model: "gpt-6-astra" })],
      }),
    );
    render(<AiSection />);
    await user.click(screen.getByRole("button", { name: "Load models" }));
    await user.selectOptions(
      await screen.findByRole("combobox", { name: "Models from the provider" }),
      "gpt-6-astra",
    );
    expect(ipc.aiUpdateProvider).toHaveBeenCalledWith("p1", {
      name: "OpenAI",
      base_url: "https://api.openai.com/v1",
      model: "gpt-6-astra",
      options: OPTIONS,
    });
    expect(screen.getByLabelText("Model")).toHaveValue("gpt-6-astra");
  });

  it("reports a refused change and puts the field back", async () => {
    const user = userEvent.setup();
    useAi({ enabled: true, active_provider: "p1", providers: [provider()] });
    vi.mocked(ipc.aiUpdateProvider).mockRejectedValue(
      new IpcError({
        code: "invalid",
        message: "The address must start with http:// or https://.",
      }),
    );
    render(<AiSection />);
    const address = screen.getByLabelText("Address");
    await user.clear(address);
    await user.type(address, "ftp://x");
    await user.tab();
    expect(await screen.findByRole("alert")).toHaveTextContent("must start with http://");
    expect(address).toHaveValue("https://api.openai.com/v1");
  });

  it("tests the connection and shows the result", async () => {
    const user = userEvent.setup();
    useAi({
      enabled: true,
      active_provider: "p1",
      providers: [provider({ has_credential: true, model: "gpt-6-astra" })],
    });
    vi.mocked(ipc.aiTestConnection).mockResolvedValue({
      ok: false,
      message: "OpenAI didn't accept the API key.",
      error: null,
    });
    render(<AiSection />);
    expect(screen.getByText(/sends a one-line request/)).toHaveTextContent("api.openai.com");
    await user.click(screen.getByRole("button", { name: "Test connection" }));
    expect(ipc.aiTestConnection).toHaveBeenCalledWith("p1");
    expect(await screen.findByRole("status")).toHaveTextContent("didn't accept the API key");
  });

  it("has no temperature for Anthropic, and shows the model's limits", async () => {
    useAi({
      enabled: true,
      active_provider: "p1",
      providers: [
        provider({
          kind: "anthropic",
          name: "Anthropic",
          base_url: "https://api.anthropic.com",
          model: "claude-x",
          has_credential: true,
        }),
      ],
    });
    vi.mocked(ipc.aiModelCapabilities).mockResolvedValue({
      tool_calling: "yes",
      context_window: 200000,
      max_output_tokens: 64000,
    });
    render(<AiSection />);
    expect(screen.queryByLabelText("Temperature")).toBeNull();
    expect(screen.getByText(/no temperature setting/)).toBeInTheDocument();
    expect(
      await screen.findByText("Context window: 200,000 tokens. Answers up to 64,000 tokens."),
    ).toBeInTheDocument();
  });

  it("asks before removing a provider and its key", async () => {
    const user = userEvent.setup();
    useAi({
      enabled: true,
      active_provider: "p1",
      providers: [provider({ has_credential: true })],
    });
    vi.mocked(ipc.aiRemoveProvider).mockResolvedValue(withAi({ enabled: true }));
    render(<AiSection />);
    await user.click(screen.getByRole("button", { name: "Remove provider" }));
    expect(ipc.aiRemoveProvider).not.toHaveBeenCalled();
    expect(screen.getByText(/delete its key from the Keychain/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Remove" }));
    expect(ipc.aiRemoveProvider).toHaveBeenCalledWith("p1");
    await waitFor(() => expect(screen.queryByLabelText("Name")).toBeNull());
  });
});
