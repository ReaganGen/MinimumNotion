// An automated accessibility pass (IMPLEMENTATION_PLAN §10.1): axe-core checks
// names, roles, labels and ARIA use on the main surfaces. Contrast can't be
// measured in jsdom; src/styles/contrast.test.ts checks it from the tokens.
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ReviewDialog } from "@/components/ai/ChangeReview";
import { AppShell } from "@/components/layout/AppShell";
import { PaletteDialog } from "@/components/search/PaletteDialog";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { type ChatTurn, resetAiChatStoreForTests, useAiChatStore } from "@/features/ai/aiStore";
import { defaultSettings } from "@/features/settings/defaults";
import { resetSettingsStoreForTests, useSettingsStore } from "@/features/settings/settingsStore";
import { type SettingsSection, useUiStore } from "@/features/ui/uiStore";
import {
  resetWorkspaceStoreForTests,
  useWorkspaceStore,
} from "@/features/workspace/workspaceStore";
import type { PageSummary } from "@/lib/ipc/bindings/PageSummary";

const PAGE = "0c6a5f3e-6c1d-4b8e-9a63-2f1d1b0a9e77";

vi.mock("@/lib/ipc/client", () => {
  const lists = new Set([
    "listChildren",
    "listFavorites",
    "listTrash",
    "listOperations",
    "listRecovery",
    "search",
    "quickOpen",
    "aiListModels",
    "backlinks",
    "resolvePages",
    "resolveWikilinks",
  ]);
  const fns = new Map<string, ReturnType<typeof vi.fn>>();
  const ipc = new Proxy(
    {},
    {
      get: (_, name: string) => {
        if (!fns.has(name)) {
          fns.set(
            name,
            vi.fn(async () => {
              if (lists.has(name)) return [];
              if (name === "aiModelCapabilities") {
                return { tool_calling: "yes", context_window: null, max_output_tokens: null };
              }
              if (name === "saveSettings") return useSettingsStore.getState().settings;
              return undefined;
            }),
          );
        }
        return fns.get(name);
      },
    },
  );
  return { ipc, assetUrl: (_: string, src: string) => src, IpcError: Error };
});

function summary(id: string, title: string, extra: Partial<PageSummary> = {}): PageSummary {
  return {
    id,
    title,
    icon: null,
    kind: "leaf",
    has_children: false,
    favorite: false,
    status: { state: "ok" },
    is_collection: false,
    ...extra,
  };
}

async function violations(): Promise<string[]> {
  // While a Radix dialog is open, it hides everything behind it with
  // aria-hidden (marked data-aria-hidden) and keeps focus inside the dialog
  // with focus guards. Both are focusable-but-hidden on purpose; axe flags
  // the pattern, so they're left out. The dialog itself is checked.
  const result = await axe.run(
    {
      include: [document.body],
      exclude: [["[data-radix-focus-guard]"], ["[data-aria-hidden]"]],
    },
    { rules: { "color-contrast": { enabled: false } }, resultTypes: ["violations"] },
  );
  return result.violations.map(
    (v) => `${v.id}: ${v.help} — ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
  );
}

function shell() {
  return render(
    <TooltipProvider>
      <AppShell />
    </TooltipProvider>,
  );
}

const ANSWER: ChatTurn = {
  id: "a1",
  role: "assistant",
  content: "Glaciers carve valleys [S1]. Also [S9].",
  status: "done",
  sources: [
    { key: "S1", kind: "passage", page_id: PAGE, title: "Glaciers", section: [], excerpt: "x" },
  ],
  activity: [{ tool: "search_pages", label: "Searched the notes for “ice”", failed: false }],
  proposal: {
    status: "pending",
    preview: {
      id: "set-1",
      request: "Tidy up",
      changes: [
        {
          change_id: "1",
          kind: "edit_page",
          page_id: PAGE,
          title: "Glaciers",
          summary: "Edit “Glaciers”",
          location: [],
          new_location: null,
          diff: [
            { kind: "delete", text: "carve" },
            { kind: "insert", text: "shape" },
          ],
          properties: [{ name: "Status", before: "To read", after: "Done" }],
          details: [],
          external_images: ["https://example.com/a.png"],
          depends_on: [],
        },
      ],
    },
  },
};

beforeEach(() => {
  resetSettingsStoreForTests();
  resetWorkspaceStoreForTests();
  resetAiChatStoreForTests();
  useUiStore.setState({ dialog: null, settingsSection: "appearance" });
});

function openWorkspace() {
  useWorkspaceStore.setState({
    status: "open",
    info: { root: "/tmp/notes", name: "notes", id: "w", read_only_reason: null, notices: [] },
    children: {
      root: [
        summary(PAGE, "Glaciers", { has_children: true }),
        summary("b", "Reading", { is_collection: true }),
      ],
    },
  });
}

function aiReady() {
  const settings = defaultSettings();
  useSettingsStore.setState({
    settings: {
      ...settings,
      layout: { ...settings.layout, ai_panel: { ...settings.layout.ai_panel, visible: true } },
      ai: {
        ...settings.ai,
        enabled: true,
        active_provider: "p1",
        providers: [
          {
            id: "p1",
            kind: "ollama",
            name: "Ollama",
            base_url: "http://localhost:11434",
            model: "llama3.2",
            has_credential: false,
            options: {
              temperature: null,
              max_output_tokens: null,
              context_window_override: null,
              tool_calling_override: null,
            },
          },
        ],
      },
    },
  });
}

describe("accessibility", () => {
  it("the welcome screen", async () => {
    useWorkspaceStore.setState({ status: "none" });
    shell();
    expect(await violations()).toEqual([]);
  });

  it("a workspace with the AI panel, an answer and a proposal", async () => {
    openWorkspace();
    aiReady();
    useAiChatStore.setState({
      turns: [
        {
          id: "q1",
          role: "user",
          content: "Tidy up",
          status: "done",
          sentTo: "Stays on this Mac · llama3.2",
          sources: [],
          activity: [],
        },
        ANSWER,
      ],
    });
    shell();
    await screen.findByRole("region", { name: "Proposed changes" });
    expect(await violations()).toEqual([]);
  });

  it.each<SettingsSection>(["appearance", "workspace", "ai", "data"])(
    "the %s settings",
    async (section) => {
      openWorkspace();
      aiReady();
      useUiStore.setState({ settingsSection: section });
      render(<SettingsDialog open onOpenChange={() => {}} />);
      await screen.findByRole("dialog");
      expect(await violations()).toEqual([]);
    },
  );

  it("the review of proposed changes", async () => {
    render(<ReviewDialog turn={ANSWER} open onOpenChange={() => {}} />);
    await screen.findByRole("dialog", { name: "Review proposed changes" });
    expect(await violations()).toEqual([]);
  });

  it("search and the AI history", async () => {
    const user = userEvent.setup();
    openWorkspace();
    aiReady();
    const { unmount } = render(
      <TooltipProvider>
        <PaletteDialog open onOpenChange={() => {}} mode="search" />
      </TooltipProvider>,
    );
    await screen.findByRole("dialog");
    expect(await violations()).toEqual([]);
    unmount();
    shell();
    await user.click(screen.getByRole("button", { name: "AI changes" }));
    await screen.findByRole("dialog", { name: "AI changes" });
    expect(await violations()).toEqual([]);
  });
});
