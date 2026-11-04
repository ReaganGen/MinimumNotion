// Development-only stand-in for the Rust backend, used when `vite dev` is opened in a
// regular browser instead of the Diaspar app. It keeps settings and a small sample
// workspace in memory (pages, a collection, links) and maps the menu accelerators to
// menu events, because a browser has no native menu. Never included in production
// builds (see main.tsx). Its search and collection logic are rough stand-ins, and
// its AI provider is fake: answers are canned and nothing goes over the network.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { CommandId } from "@/features/commands/commands";
import { defaultSettings } from "@/features/settings/defaults";
import type { AiSettings } from "./bindings/AiSettings";
import type { Cell } from "./bindings/Cell";
import type { CellInput } from "./bindings/CellInput";
import type { ChangeOutcome } from "./bindings/ChangeOutcome";
import type { ChangeSetPreview } from "./bindings/ChangeSetPreview";
import type { ChatInput } from "./bindings/ChatInput";
import type { ChatStreamEvent } from "./bindings/ChatStreamEvent";
import type { CollectionRow } from "./bindings/CollectionRow";
import type { CollectionViews } from "./bindings/CollectionViews";
import type { OperationInfo } from "./bindings/OperationInfo";
import type { PageDocument } from "./bindings/PageDocument";
import type { PageHit } from "./bindings/PageHit";
import type { PageSummary } from "./bindings/PageSummary";
import type { ProviderKind } from "./bindings/ProviderKind";
import type { ProviderUpdate } from "./bindings/ProviderUpdate";
import type { Schema } from "./bindings/Schema";
import type { SchemaOp } from "./bindings/SchemaOp";
import type { SettingsUpdate } from "./bindings/SettingsUpdate";
import type { Source } from "./bindings/Source";
import { MENU_EVENT } from "./client";

const ACCELERATORS: Record<string, CommandId> = {
  "meta+n": "new-page",
  "meta+shift+l": "toggle-sidebar",
  "meta+shift+a": "toggle-ai-panel",
  "meta+,": "open-settings",
  "meta+k": "open-search",
  "meta+p": "open-quick-open",
};

type Value = string | number | boolean | string[];

interface MemoryPage {
  id: string;
  parent: string | null;
  title: string;
  body: string;
  rev: number;
  schema?: Schema;
  values: Record<string, Value>;
}

const id = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;

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

Inline math $e^{i\\pi} + 1 = 0$. A page link: [Paper notes](diaspar://page/${id(3)}), and a wikilink: [[Reading list]].
`;

function memoryWorkspace() {
  const pages = new Map<string, MemoryPage>();
  const add = (pageId: string, parent: string | null, title: string, body = "") =>
    pages.set(pageId, { id: pageId, parent, title, body, rev: 1, values: {} });
  add(id(1), null, "Welcome", SAMPLE_BODY);
  add(id(2), null, "Research");
  add(id(3), id(2), "Paper notes", "Notes on transformers and protein design.\n");
  add(id(4), null, "Reading list", "Papers to read.\n");
  (pages.get(id(4)) as MemoryPage).schema = {
    version: 1,
    properties: [
      {
        id: "status00",
        name: "Status",
        type: "select",
        options: [
          { id: "tord", name: "To read", color: "gray" },
          { id: "rdng", name: "Reading", color: "blue" },
          { id: "done", name: "Done", color: "green" },
        ],
      },
      { id: "year0000", name: "Year", type: "number", options: [] },
      { id: "read0000", name: "Read", type: "checkbox", options: [] },
    ],
  };
  const entries: [string, Value, number, boolean][] = [
    ["Attention Is All You Need", "Reading", 2017, false],
    ["BERT", "To read", 2018, false],
    ["AlphaFold 2", "Done", 2021, true],
  ];
  entries.forEach(([title, status, year, read], i) => {
    add(id(5 + i), id(4), title, "");
    (pages.get(id(5 + i)) as MemoryPage).values = { Status: status, Year: year, Read: read };
  });
  let counter = 10;
  const views = new Map<string, CollectionViews>();

  const get = (pageId: string) => {
    const page = pages.get(pageId);
    if (!page) throw { code: "not_found", message: "This page wasn't found." };
    return page;
  };
  const summary = (p: MemoryPage): PageSummary => ({
    id: p.id,
    title: p.title,
    icon: null,
    kind: "leaf",
    has_children: [...pages.values()].some((c) => c.parent === p.id),
    favorite: false,
    status: { state: "ok" },
    is_collection: Boolean(p.schema),
  });
  const ancestors = (p: MemoryPage) => {
    const out = [];
    for (let cursor = p.parent; cursor; cursor = pages.get(cursor)?.parent ?? null) {
      const a = pages.get(cursor);
      if (a) out.unshift({ id: a.id, title: a.title, icon: null });
    }
    return out;
  };
  const bodyRev = (p: MemoryPage) => `${p.id}:${p.rev}`;
  const doc = (p: MemoryPage): PageDocument => ({
    ...summary(p),
    parent_id: p.parent,
    tags: [],
    created_at: null,
    updated_at: null,
    body: p.body,
    rev: bodyRev(p),
    body_rev: bodyRev(p),
    ancestors: ancestors(p),
    collection_id: p.parent && pages.get(p.parent)?.schema ? p.parent : null,
  });
  const hit = (p: MemoryPage, snippet: string | null = null): PageHit => ({
    id: p.id,
    title: p.title,
    icon: null,
    path: ancestors(p).map((a) => a.title),
    snippet,
  });

  const cell = (schema: Schema, propertyId: string, value: Value | undefined): Cell => {
    const property = schema.properties.find((p) => p.id === propertyId);
    if (value === undefined || !property) return { kind: "empty" };
    switch (property.type) {
      case "number":
        return { kind: "number", value: value as number, from_text: false };
      case "checkbox":
        return { kind: "checkbox", value: value as boolean };
      case "select":
        return {
          kind: "select",
          value: value as string,
          known: property.options.some((o) => o.name === value),
        };
      case "multi_select":
        return { kind: "multi_select", values: value as string[], unknown: [] };
      case "date":
        return { kind: "date", value: value as string, from_timestamp: false };
      default:
        return { kind: "text", value: String(value) };
    }
  };
  const row = (p: MemoryPage, schema: Schema): CollectionRow => ({
    id: p.id,
    title: p.title,
    icon: null,
    created_at: null,
    updated_at: null,
    editable: true,
    cells: Object.fromEntries(
      schema.properties.map((prop) => [prop.id, cell(schema, prop.id, p.values[prop.name])]),
    ),
  });
  const collectionOf = (pageId: string) => {
    const page = get(pageId);
    if (!page.schema) throw { code: "invalid", message: "This page isn't a collection." };
    return { page, schema: page.schema };
  };
  const changed = () =>
    void emit("workspace://changed", { content_changed: [], removed: [], tree_changed: true });

  return {
    info: {
      root: "/preview/Diaspar",
      name: "Diaspar (preview)",
      id: "preview",
      read_only_reason: null,
      notices: [],
    },
    children: (parent: string | null) =>
      [...pages.values()].filter((p) => p.parent === parent).map(summary),
    get: (pageId: string) => doc(get(pageId)),
    save: (pageId: string, base: string | null, body: string) => {
      const page = get(pageId);
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
      add(id(counter), parent, title);
      changed();
      return summary(get(id(counter)));
    },
    rename: (pageId: string, title: string) => {
      get(pageId).title = title;
      changed();
    },
    search: (query: string) => {
      const q = query.toLowerCase();
      return [...pages.values()]
        .filter((p) => `${p.title}\n${p.body}`.toLowerCase().includes(q))
        .map((p) => {
          const at = p.body.toLowerCase().indexOf(q);
          if (at < 0) return hit(p);
          const before = p.body.slice(Math.max(0, at - 20), at);
          const match = p.body.slice(at, at + q.length);
          const after = p.body.slice(at + q.length, at + q.length + 40);
          return hit(p, `${before}\u0001${match}\u0002${after}`);
        });
    },
    /** Pages sharing a word of four or more letters with `query`: a stand-in for retrieval. */
    related: (query: string, exclude: string | null) => {
      const words = query.toLowerCase().match(/[\p{L}\p{N}]{4,}/gu) ?? [];
      return [...pages.values()].filter(
        (p) =>
          p.id !== exclude &&
          p.body.trim() !== "" &&
          words.some((w) => `${p.title}\n${p.body}`.toLowerCase().includes(w)),
      );
    },
    page: (pageId: string) => pages.get(pageId) ?? null,
    /** Edit mode stand-ins: write a body, add or remove a page. */
    setBody: (pageId: string, body: string) => {
      const page = get(pageId);
      page.body = body;
      page.rev += 1;
      void emit("workspace://changed", {
        content_changed: [pageId],
        removed: [],
        tree_changed: false,
      });
    },
    addPage: (title: string, body: string) => {
      counter += 1;
      add(id(counter), null, title, body);
      changed();
      return id(counter);
    },
    removePage: (pageId: string) => {
      pages.delete(pageId);
      void emit("workspace://changed", {
        content_changed: [],
        removed: [pageId],
        tree_changed: true,
      });
    },
    quickOpen: (query: string) =>
      [...pages.values()]
        .filter((p) => p.title.toLowerCase().includes(query.toLowerCase()))
        .map((p) => hit(p)),
    resolvePages: (ids: string[]) =>
      ids.map((pageId) => {
        const page = pages.get(pageId);
        return page
          ? { id: pageId, title: page.title, icon: null, state: "ok" }
          : { id: pageId, title: null, icon: null, state: "missing" };
      }),
    resolveWikilinks: (texts: string[]) =>
      texts.map((text) => {
        const found = [...pages.values()].filter(
          (p) => p.title.toLowerCase() === text.toLowerCase(),
        );
        const only = found.length === 1 ? found[0] : undefined;
        return {
          text,
          target: only ? { id: only.id, title: only.title, icon: null } : null,
          matches: found.length,
        };
      }),
    backlinks: (pageId: string) =>
      [...pages.values()].filter((p) => p.body.includes(pageId)).map((p) => hit(p)),
    collection: (pageId: string) => {
      const { page, schema } = collectionOf(pageId);
      const rows = [...pages.values()]
        .filter((p) => p.parent === pageId)
        .map((p) => row(p, schema));
      return { id: pageId, title: page.title, schema, problem: null, rows };
    },
    entry: (pageId: string) => {
      const page = get(pageId);
      const parent = page.parent ? pages.get(page.parent) : undefined;
      if (!parent?.schema) return null;
      return {
        collection_id: parent.id,
        collection_title: parent.title,
        schema: parent.schema,
        problem: null,
        row: row(page, parent.schema),
      };
    },
    setProperty: (pageId: string, propertyId: string, value: CellInput) => {
      const page = get(pageId);
      const { schema } = collectionOf(page.parent ?? "");
      const property = schema.properties.find((p) => p.id === propertyId);
      if (!property) throw { code: "not_found", message: "This property wasn't found." };
      if (value.kind === "empty") delete page.values[property.name];
      else if (value.kind === "multi_select") page.values[property.name] = value.values;
      else page.values[property.name] = value.value;
    },
    updateSchema: (pageId: string, op: SchemaOp) => {
      const { schema } = collectionOf(pageId);
      const property =
        "property_id" in op ? schema.properties.find((p) => p.id === op.property_id) : undefined;
      switch (op.op) {
        case "add_property":
          counter += 1;
          schema.properties.push({
            id: `prop${String(counter).padStart(4, "0")}`,
            name: op.name,
            type: op.type,
            options: [],
          });
          break;
        case "rename_property":
          if (property) property.name = op.name;
          break;
        case "set_property_type":
          if (property) property.type = op.type;
          break;
        case "remove_property":
          schema.properties = schema.properties.filter((p) => p.id !== op.property_id);
          break;
        case "add_option":
          property?.options.push({
            id: `o${property.options.length}xx`,
            name: op.name,
            color: "purple",
          });
          break;
        default:
          throw { code: "internal", message: `Not available in browser preview: ${op.op}` };
      }
      return schema;
    },
    views: (pageId: string): CollectionViews =>
      views.get(pageId) ?? {
        active_view: "table",
        table: { sort: [], filter: [], hidden: [], column_widths: {} },
        list: { sort: [], filter: [], hidden: [], column_widths: {} },
      },
    saveViews: (pageId: string, next: CollectionViews) => views.set(pageId, next),
  };
}

const PRESETS: Record<ProviderKind, { name: string; base_url: string }> = {
  anthropic: { name: "Anthropic", base_url: "https://api.anthropic.com" },
  openai: { name: "OpenAI", base_url: "https://api.openai.com/v1" },
  openai_compatible: { name: "OpenAI-compatible", base_url: "http://localhost:8000/v1" },
  ollama: { name: "Ollama", base_url: "http://localhost:11434" },
  lmstudio: { name: "LM Studio", base_url: "http://localhost:1234/v1" },
};

const CANNED_ANSWER = `This is the **browser preview**, so no model was asked. In the app, the answer streams here.

- Markdown renders: \`code\`, *emphasis*, [a link](https://example.com)
- Raw HTML shows as text: <b>not bold</b>
- Images aren't loaded: ![chart](https://example.com/chart.png)`;

type PreviewWorkspace = ReturnType<typeof memoryWorkspace>;

/** Roughly the plain text Rust would extract: links as their labels, no markup. */
function excerpt(text: string): string {
  const flat = text
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[*_`#>|]/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return flat.length > 240 ? `${flat.slice(0, 240)}…` : flat;
}

/** An in-memory stand-in for the AI commands. Never touches the network. */
function memoryAi(getAi: () => AiSettings, setAi: (ai: AiSettings) => void) {
  const running = new Map<string, () => void>();
  const proposals = new Map<string, (chosen: string[]) => ChangeOutcome>();
  const operations: OperationInfo[] = [];
  const undos = new Map<string, () => void>();
  const update = (change: (ai: AiSettings) => AiSettings) => setAi(change(getAi()));
  const withProvider = (id: string, change: (p: AiSettings["providers"][number]) => void) =>
    update((ai) => {
      const providers = ai.providers.map((p) => ({ ...p }));
      const provider = providers.find((p) => p.id === id);
      if (provider) change(provider);
      return { ...ai, providers };
    });

  return {
    add(kind: ProviderKind) {
      const id = crypto.randomUUID();
      update((ai) => ({
        ...ai,
        active_provider: ai.active_provider ?? id,
        providers: [
          ...ai.providers,
          {
            id,
            kind,
            ...PRESETS[kind],
            model: "",
            has_credential: false,
            options: {
              temperature: null,
              max_output_tokens: null,
              context_window_override: null,
              tool_calling_override: null,
            },
          },
        ],
      }));
    },
    update: (id: string, u: ProviderUpdate) => withProvider(id, (p) => Object.assign(p, u)),
    remove: (id: string) =>
      update((ai) => {
        const providers = ai.providers.filter((p) => p.id !== id);
        return {
          ...ai,
          providers,
          active_provider:
            ai.active_provider === id ? (providers[0]?.id ?? null) : ai.active_provider,
        };
      }),
    setKey: (id: string, stored: boolean) =>
      withProvider(id, (p) => {
        p.has_credential = stored;
      }),
    chat(
      requestId: string,
      input: ChatInput,
      ws: PreviewWorkspace,
      onEvent: (event: ChatStreamEvent) => void,
    ): Promise<null> {
      const { context } = input;
      const question = input.messages.at(-1)?.content ?? "";
      const sources: Source[] = [];
      const add = (kind: Source["kind"], page: { id: string; title: string }, text: string) =>
        sources.push({
          key: `S${context.first_source + sources.length}`,
          kind,
          page_id: page.id,
          title: page.title,
          section: [],
          excerpt: excerpt(text),
        });
      const attached = context.page_id ? ws.page(context.page_id) : null;
      if (context.selection) {
        const from = ws.page(context.selection.page_id);
        if (from) add("selection", from, context.selection.text);
      }
      if (attached) add("page", attached, attached.body);
      const found = context.search_notes
        ? ws.related(question, attached?.id ?? null).slice(0, 4)
        : [];
      for (const page of found) add("passage", page, page.body);
      onEvent({
        type: "context",
        context: {
          page: attached ? { id: attached.id, title: attached.title } : null,
          page_complete: true,
          selection: context.selection !== null,
          selection_complete: true,
          searched: context.search_notes,
          passages: found.length,
          left_out_turns: 0,
          tools: false,
          edit: context.edit,
        },
      });
      if (sources.length > 0) onEvent({ type: "sources", sources });
      const cited =
        sources.length > 0
          ? `\n\nFrom your notes: “${sources[0]?.excerpt.slice(0, 60)}” [${sources[0]?.key}]. A key the model made up shows as unverified [S99].`
          : "";
      const proposal = context.edit ? this.propose(ws, attached ?? ws.page(id(1))) : null;
      const text = proposal
        ? `This is the **browser preview**, so no model was asked. It proposes a new page and an edit to “${proposal.changes[1]?.title}”, for you to review below. Nothing changes until you apply them.`
        : CANNED_ANSWER + cited;
      if (proposal) onEvent({ type: "proposal", proposal });
      const words = text.split(/(?<= )/);
      return new Promise((resolve) => {
        let i = 0;
        const timer = setInterval(() => {
          if (i < words.length) {
            onEvent({ type: "text", text: words[i++] ?? "" });
            return;
          }
          clearInterval(timer);
          running.delete(requestId);
          onEvent({ type: "done", stop_reason: "end_turn" });
          resolve(null);
        }, 40);
        running.set(requestId, () => {
          clearInterval(timer);
          running.delete(requestId);
          onEvent({
            type: "failed",
            error: { kind: "cancelled", message: "Stopped.", status: null, retry_after_secs: null },
          });
          resolve(null);
        });
      });
    },
    cancel: (requestId: string) => running.get(requestId)?.(),

    /** A made-up proposal: a new page with an external image, and an edit. */
    propose(ws: PreviewWorkspace, target: { id: string; title: string; body: string } | null) {
      if (!target) return null;
      const added = "\n\nAdded in the preview, to show an edit.\n";
      const preview: ChangeSetPreview = {
        id: crypto.randomUUID(),
        request: "Preview request",
        changes: [
          {
            change_id: "1",
            kind: "create_page",
            page_id: null,
            title: "Ideas from the preview",
            summary: "Create the page “Ideas from the preview” at the top level",
            location: [],
            new_location: null,
            diff: [
              {
                kind: "insert",
                text: "- A first idea\n- ![chart](https://example.com/chart.png)\n",
              },
            ],
            properties: [],
            details: [],
            external_images: ["https://example.com/chart.png"],
            depends_on: [],
          },
          {
            change_id: "2",
            kind: "edit_page",
            page_id: target.id,
            title: target.title,
            summary: `Edit “${target.title}”`,
            location: [],
            new_location: null,
            diff: [
              { kind: "equal", text: target.body.trimEnd() },
              { kind: "insert", text: added },
            ],
            properties: [],
            details: [],
            external_images: [],
            depends_on: [],
          },
        ],
      };
      const before = target.body;
      proposals.set(preview.id, (chosen: string[]) => {
        const page = ws.page(target.id);
        if (chosen.includes("2") && page?.body !== before) {
          return {
            outcome: "conflicts",
            conflicts: [
              {
                change_id: "2",
                title: target.title,
                message: "Its text changed after this was proposed.",
              },
            ],
          } satisfies ChangeOutcome;
        }
        const undo: (() => void)[] = [];
        let summary = [];
        if (chosen.includes("1")) {
          const created = ws.addPage(
            "Ideas from the preview",
            "- A first idea\n- ![chart](https://example.com/chart.png)\n",
          );
          undo.push(() => ws.removePage(created));
          summary.push("created 1 page");
        }
        if (chosen.includes("2")) {
          ws.setBody(target.id, `${before.trimEnd()}${added}`);
          undo.push(() => ws.setBody(target.id, before));
          summary.push("edited 1 page");
        }
        summary = summary.map((s, i) => (i === 0 ? s[0]?.toUpperCase() + s.slice(1) : s));
        const operation: OperationInfo = {
          op_id: crypto.randomUUID(),
          applied_at: new Date().toISOString(),
          source: "ai",
          request: preview.request,
          summary: summary.join(", "),
          model: "preview",
          undoes: null,
          undone: false,
          pages: [{ id: target.id, title: target.title }],
        };
        operations.unshift(operation);
        undos.set(operation.op_id, () => {
          for (const step of undo.reverse()) step();
        });
        return { outcome: "applied", operation } satisfies ChangeOutcome;
      });
      return preview;
    },
    apply: (changesetId: string, changeIds: string[]): ChangeOutcome => {
      const apply = proposals.get(changesetId);
      if (!apply)
        throw { code: "invalid", message: "This proposal is no longer available. Ask again." };
      const outcome = apply(changeIds);
      if (outcome.outcome === "applied") proposals.delete(changesetId);
      return outcome;
    },
    reject: (changesetId: string) => proposals.delete(changesetId),
    operations: () => operations,
    undo: (opId: string): ChangeOutcome => {
      const op = operations.find((o) => o.op_id === opId);
      const undo = undos.get(opId);
      if (!op || !undo || op.undone) {
        throw { code: "invalid", message: "The browser preview can't redo changes." };
      }
      undo();
      undos.delete(opId);
      op.undone = true;
      const undoOp: OperationInfo = {
        ...op,
        op_id: crypto.randomUUID(),
        applied_at: new Date().toISOString(),
        source: "undo",
        summary: `Undid: ${op.summary}`,
        undoes: op.op_id,
        undone: false,
      };
      operations.unshift(undoOp);
      return { outcome: "applied", operation: undoOp };
    },
  };
}

export function installBrowserPreview() {
  let settings = defaultSettings();
  let notice: string | null =
    "Browser preview: running without the Diaspar app. Nothing is saved to disk.";
  const ws = memoryWorkspace();
  const ai = memoryAi(
    () => settings.ai,
    (next) => {
      settings = { ...settings, ai: next };
    },
  );

  mockWindows("main");
  mockIPC(
    (command, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      const str = (key: string) => a[key] as string;
      switch (command) {
        case "get_settings": {
          const snapshot = { settings, notice, read_only_reason: null };
          notice = null;
          return snapshot;
        }
        case "save_settings":
          settings = { ...settings, ...(a.update as SettingsUpdate) };
          return settings;
        case "ai_set_enabled":
          settings = { ...settings, ai: { ...settings.ai, enabled: a.enabled as boolean } };
          return settings;
        case "ai_add_provider":
          ai.add(a.kind as ProviderKind);
          return settings;
        case "ai_update_provider":
          ai.update(str("id"), a.update as ProviderUpdate);
          return settings;
        case "ai_remove_provider":
          ai.remove(str("id"));
          return settings;
        case "ai_set_active_provider":
          settings = { ...settings, ai: { ...settings.ai, active_provider: str("id") } };
          return settings;
        case "ai_set_api_key":
        case "ai_remove_api_key":
          ai.setKey(str("id"), command === "ai_set_api_key");
          return settings;
        case "ai_acknowledge_cloud":
          settings = {
            ...settings,
            ai: {
              ...settings.ai,
              cloud_consent: {
                ...settings.ai.cloud_consent,
                [str("id")]: new Date().toISOString(),
              },
            },
          };
          return settings;
        case "ai_list_models":
          return [
            { id: "preview-small", name: "Preview Small", details: null },
            { id: "preview-large", name: "Preview Large", details: null },
          ];
        case "ai_model_capabilities":
          return { tool_calling: "unknown", context_window: 32768, max_output_tokens: null };
        case "ai_test_connection":
          return { ok: true, message: "Browser preview: no request was sent.", error: null };
        case "ai_chat": {
          const channel = a.onEvent as { onmessage: (event: ChatStreamEvent) => void };
          return ai.chat(str("requestId"), a.input as ChatInput, ws, (event) =>
            channel.onmessage(event),
          );
        }
        case "ai_cancel":
          ai.cancel(str("requestId"));
          return null;
        case "apply_changeset":
          return ai.apply(str("changesetId"), a.changeIds as string[]);
        case "reject_changeset":
          ai.reject(str("changesetId"));
          return null;
        case "list_operations":
          return ai.operations();
        case "undo_operation":
          return ai.undo(str("opId"));
        case "workspace_open_last":
        case "workspace_info":
          return ws.info;
        case "list_children":
          return ws.children((a.parentId as string | null) ?? null);
        case "get_page":
          return ws.get(str("id"));
        case "save_page":
          return ws.save(str("id"), a.baseBodyRev as string | null, str("body"));
        case "create_page":
          return ws.create((a.parentId as string | null) ?? null, str("title"));
        case "set_title":
          ws.rename(str("id"), str("title"));
          return null;
        case "search":
          return ws.search(str("query"));
        case "quick_open":
          return ws.quickOpen(str("query"));
        case "resolve_pages":
          return ws.resolvePages(a.ids as string[]);
        case "resolve_wikilinks":
          return ws.resolveWikilinks(a.texts as string[]);
        case "backlinks":
          return ws.backlinks(str("id"));
        case "get_collection":
          return ws.collection(str("id"));
        case "get_entry":
          return ws.entry(str("id"));
        case "set_property":
          ws.setProperty(str("pageId"), str("propertyId"), a.value as CellInput);
          return null;
        case "update_schema":
          return ws.updateSchema(str("id"), a.op as SchemaOp);
        case "get_views":
          return ws.views(str("id"));
        case "save_views":
          ws.saveViews(str("id"), a.views as CollectionViews);
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
        case "ensure_persisted_id":
        case "rebuild_index":
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
