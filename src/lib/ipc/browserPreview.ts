// Development-only stand-in for the Rust backend, used when `vite dev` is opened in a
// regular browser instead of the Forma app. It keeps settings and a small sample
// workspace in memory and maps the menu accelerators to menu events, because a
// browser has no native menu. Never included in production builds (see main.tsx).
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { CommandId } from "@/features/commands/commands";
import { defaultSettings } from "@/features/settings/defaults";
import type { PageDocument } from "./bindings/PageDocument";
import type { PageSummary } from "./bindings/PageSummary";
import type { SettingsUpdate } from "./bindings/SettingsUpdate";
import { MENU_EVENT } from "./client";

const ACCELERATORS: Record<string, CommandId> = {
  "meta+n": "new-page",
  "meta+shift+l": "toggle-sidebar",
  "meta+shift+a": "toggle-ai-panel",
  "meta+,": "open-settings",
  "meta+k": "open-search",
  "meta+p": "open-quick-open",
};

interface MemoryPage {
  id: string;
  parent: string | null;
  title: string;
  body: string;
  rev: number;
}

const SAMPLE_BODY = `This is a **preview** page served from memory. Try *formatting*, \`code\`, and lists:

- First item
- Second item
  - Nested

- [ ] A task
- [x] A done task

> A quote.

| Name | Value |
| --- | --- |
| a | 1 |

Inline math $e^{i\\pi} + 1 = 0$.
`;

function memoryWorkspace() {
  const pages = new Map<string, MemoryPage>();
  const add = (id: string, parent: string | null, title: string, body = "") =>
    pages.set(id, { id, parent, title, body, rev: 1 });
  add("00000000-0000-4000-8000-000000000001", null, "Welcome", SAMPLE_BODY);
  add("00000000-0000-4000-8000-000000000002", null, "Research");
  add(
    "00000000-0000-4000-8000-000000000003",
    "00000000-0000-4000-8000-000000000002",
    "Paper notes",
    "Notes.\n",
  );
  let counter = 10;

  const summary = (p: MemoryPage): PageSummary => ({
    id: p.id,
    title: p.title,
    icon: null,
    kind: "leaf",
    has_children: [...pages.values()].some((c) => c.parent === p.id),
    favorite: false,
    status: { state: "ok" },
  });
  const bodyRev = (p: MemoryPage) => `${p.id}:${p.rev}`;
  const doc = (p: MemoryPage): PageDocument => {
    const ancestors = [];
    for (let cursor = p.parent; cursor; cursor = pages.get(cursor)?.parent ?? null) {
      const a = pages.get(cursor);
      if (a) ancestors.unshift({ id: a.id, title: a.title, icon: null });
    }
    return {
      ...summary(p),
      parent_id: p.parent,
      tags: [],
      created_at: null,
      updated_at: null,
      body: p.body,
      rev: bodyRev(p),
      body_rev: bodyRev(p),
      ancestors,
    };
  };

  return {
    info: {
      root: "/preview/Forma",
      name: "Forma (preview)",
      id: "preview",
      read_only_reason: null,
      notices: [],
    },
    children: (parent: string | null) =>
      [...pages.values()].filter((p) => p.parent === parent).map(summary),
    get: (id: string) => {
      const page = pages.get(id);
      if (!page) throw { code: "not_found", message: "This page wasn't found." };
      return doc(page);
    },
    save: (id: string, base: string | null, body: string) => {
      const page = pages.get(id);
      if (!page) throw { code: "not_found", message: "This page wasn't found." };
      if (base !== bodyRev(page))
        return { outcome: "conflict", current_body_rev: bodyRev(page), current_body: page.body };
      page.body = body;
      page.rev += 1;
      return {
        outcome: "saved",
        rev: bodyRev(page),
        body_rev: bodyRev(page),
        updated_at: new Date().toISOString(),
      };
    },
    create: (parent: string | null, title: string) => {
      counter += 1;
      const id = `00000000-0000-4000-8000-${String(counter).padStart(12, "0")}`;
      add(id, parent, title);
      return summary(pages.get(id) as MemoryPage);
    },
    rename: (id: string, title: string) => {
      const page = pages.get(id);
      if (page) page.title = title;
    },
  };
}

export function installBrowserPreview() {
  let settings = defaultSettings();
  let notice: string | null =
    "Browser preview: running without the Forma app. Nothing is saved to disk.";
  const workspace = memoryWorkspace();

  mockWindows("main");
  mockIPC(
    (command, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (command) {
        case "get_settings": {
          const snapshot = { settings, notice, read_only_reason: null };
          notice = null;
          return snapshot;
        }
        case "save_settings":
          settings = { ...settings, ...(a.update as SettingsUpdate) };
          return settings;
        case "workspace_open_last":
        case "workspace_info":
          return workspace.info;
        case "list_children":
          return workspace.children((a.parentId as string | null) ?? null);
        case "get_page":
          return workspace.get(a.id as string);
        case "save_page":
          return workspace.save(a.id as string, a.baseBodyRev as string | null, a.body as string);
        case "create_page":
          return workspace.create((a.parentId as string | null) ?? null, a.title as string);
        case "set_title":
          workspace.rename(a.id as string, a.title as string);
          void emit("workspace://changed", {
            content_changed: [],
            removed: [],
            tree_changed: true,
          });
          return null;
        case "list_favorites":
        case "list_trash":
        case "list_recovery":
        case "take_notices":
          return [];
        case "apply_window_theme":
        case "app_ready":
        case "write_recovery":
        case "discard_recovery":
          return null;
        default:
          throw { code: "internal", message: `Not available in browser preview: ${command}` };
      }
    },
    { shouldMockEvents: true },
  );

  window.addEventListener("keydown", (event) => {
    const combo = [event.metaKey && "meta", event.shiftKey && "shift", event.key.toLowerCase()]
      .filter(Boolean)
      .join("+");
    const command = ACCELERATORS[combo];
    if (command) {
      event.preventDefault();
      void emit(MENU_EVENT, command);
    }
  });
}
