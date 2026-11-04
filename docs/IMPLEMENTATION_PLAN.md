# Diaspar Implementation Plan

Status: Phase 0 baseline (2026-11-02). Each phase follows the cycle **Plan → Implement → Test → Review → Report → Stop**. A phase starts only after the user explicitly says "Proceed to Phase N" (see [CLAUDE.md](../CLAUDE.md)). A phase is complete only when every acceptance criterion below has been verified by commands that were actually run or by documented manual checks.

---

## 1. Prerequisites (toolchain)

On 2026-11-02 the development machine (macOS 26.3, Apple Silicon) had Xcode Command Line Tools, git 2.50 and Homebrew, but **no Node.js and no Rust toolchain**. Phase 1 can't start until they are installed, which needs the user's go-ahead:

| Tool | Requirement | Install |
|---|---|---|
| Xcode Command Line Tools | Present (`/Library/Developer/CommandLineTools`). Enough for Tauri desktop builds. Full Xcode isn't required | — |
| Rust | stable ≥ 1.90 (`tauri` 2.12 MSRV), via rustup. **Installed in Phase 1: 1.99.1**, plus the `x86_64-apple-darwin` target | Official installer from https://rustup.rs |
| Rust targets | `aarch64-apple-darwin` (default). `x86_64-apple-darwin` is needed for the universal build in Phase 7 | `rustup target add x86_64-apple-darwin` |
| Node.js | ≥ 24 LTS. **Installed in Phase 1: 26.13.0 LTS** (Homebrew `node`, which is linked onto `PATH`) with npm 11.20 | `brew install node` (26 LTS), or `node@24`, nvm or fnm |
| npm | Bundled with Node | — |

---

## 2. Verified dependency versions (2026-11-02)

These were checked against the npm registry and crates.io on 2026-11-02. Each phase pins exact versions in the lockfiles when it adds the dependency. Re-check before adding a dependency that the current phase doesn't introduce.

**Frontend**

| Package | Version | Notes |
|---|---|---|
| `@tauri-apps/cli`, `@tauri-apps/api` | 2.12.4 | Tauri 3 is in alpha, so not used |
| `react`, `react-dom` | 19.3.1 | |
| `typescript` | 6.0.3 | TypeScript 7 isn't used (ADR-016) |
| `vite` | 8.3.5 | Needs Node `^20.19 \|\| >=22.12` |
| `@vitejs/plugin-react` | 6.2.1 | Peer `vite ^8` |
| `tailwindcss`, `@tailwindcss/vite` | 4.3.4 | Peer `vite ^5–^8` |
| `shadcn` (CLI) | 4.23.0 | Run as `npx shadcn@4.23.0 add …`; **not** a project dependency, because its own dependencies carry npm audit findings. Generated components live in `src/components/ui/` and use `radix-ui` 1.8, `class-variance-authority` 0.7.1, `clsx` 2.1.1, `tailwind-merge` 3.7.0 and `tw-animate-css` 1.4.0 |
| `lucide-react` | 1.66.0 | |
| `@tiptap/core`, `/react`, `/pm`, `/starter-kit`, `/markdown`, `/extension-*` | 3.32.2 | `@tiptap/markdown` uses `marked` 17 |
| `katex` | 0.18.11 | Pinned below 0.19: `@tiptap/extension-mathematics` peer `^0.16 \|\| ^0.17 \|\| ^0.18` |
| `zustand` | 5.0.16 | |
| `react-resizable-panels` | 4.16.1 | Not used: custom resize handles instead (ADR-029) |
| `cmdk` | 1.1.1 | Quick open and command palette |
| `@tanstack/react-virtual` | 3.14.25 | |
| `vitest` | 5.1.0 | Peer `vite ^6.4 \|\| ^7 \|\| ^8` |
| `@testing-library/react` | 16.3.3 | Plus `@testing-library/dom` 10 |
| `jsdom` | 30.1.3 | Vitest environment |
| `@biomejs/biome` | 2.6.4 | Lint and format |
| `@tauri-apps/plugin-dialog`, `-opener`, `-window-state` | 2.8.4 / 2.7.2 / 2.5.0 | |

**Rust**

| Crate | Version | Notes |
|---|---|---|
| `tauri` / `tauri-build` | 2.12.5 / 2.7.4 | MSRV 1.90 |
| `tauri-plugin-dialog`, `-opener`, `-window-state`, `-single-instance`, `-log` | 2.8.4 / 2.7.2 / 2.5.0 / 2.5.5 / 2.10.1 | No `tauri-plugin-fs` (ARCHITECTURE §2.1) |
| `rusqlite` | 0.41.1, feature `bundled` | `libsqlite3-sys` 0.39. The bundled build sets `SQLITE_ENABLE_FTS5`, which Phase 3 confirms with a test |
| `uuid` | 1.29.0 (`v4`, `serde`) | |
| `blake3` | 1.8.7 | Revisions |
| `notify` / `notify-debouncer-full` | 8.2.1 / 0.7.0 | FSEvents backend |
| `saphyr` (+ `saphyr-parser`) | 0.3.x | Span-aware YAML (ADR-028). `serde_yaml` is deprecated |
| `pulldown-cmark` | 0.13.6 | Text and link extraction |
| `similar` | 3.2.1, feature `unicode` | Diffs for AI previews; `unicode` splits words by Unicode rules (CJK per character). Re-checked 2026-11-03: the latest release, Apache-2.0 |
| `serde` / `serde_json` | 1.0.229 / 1.0.154 | |
| `thiserror` | 2.0.21 | |
| `chrono` | 0.4.45 | RFC 3339 timestamps |
| `ts-rs` | 12.0.1 | IPC type generation (ADR-023) |
| `tokio` | 1.54.1 | |
| `reqwest` | 0.13.5 | `diaspar-ai` only. Features `native-tls` (macOS Security framework and the system's certificates), `http2`, `json`, `stream`, `charset`, `system-proxy` |
| `keyring-core` / `apple-native-keyring-store` | 1.0.1 / 1.0.2 | ADR-020. The store needs its `keychain` feature (the login keychain) |
| `tokio-util` / `url` | 0.7.20 / 2.5.8 | `CancellationToken`; base-URL checks |
| `marked` (npm) | 17.0.6 | Already installed through `@tiptap/markdown`; a direct dependency at the same version since Phase 4, to tokenize chat answers (ADR-040) |
| `tempfile` | 3.27.0 | Tests only (production temp files go in `.diaspar/tmp/`) |
| `zip` | 9.3.0, `default-features = false`, feature `deflate-flate2-zlib-rs` | Phase 7 backup and restore (ADR-044). MIT. Pulls in `flate2` with `zlib-rs` 0.6.8 (pure Rust, Zlib license); no other compression formats. Checked 2026-11-03 |
| `axe-core` (npm, dev) | 4.14.0 | Phase 7 accessibility tests (`src/a11y.test.tsx`). MPL-2.0, released 2026-10-05; used only in tests, never bundled |
| `cargo-audit` (tool) | 0.22.2 | Installed by CI with `cargo install --locked`, not a dependency. Released 2026-06-05 |
| GitHub Actions | `actions/checkout` 7.0.1, `actions/setup-node` 7.1.0, `actions/upload-artifact` 7.0.2, `Swatinem/rust-cache` 2.9.3, `softprops/action-gh-release` 3.0.3 | Pinned to commit SHAs in `.github/workflows/`. Runners `macos-26` (arm64), and `ubuntu-24.04` for the dependency audit. Checked 2026-11-03 |

---

## 3. Phase 0: Architecture and Project Planning

**Deliverables (this phase):**

- `CLAUDE.md`
- `docs/ARCHITECTURE.md`, `DATA_MODEL.md`, `DECISIONS.md`, `IMPLEMENTATION_PLAN.md`, `PROGRESS.md`, `ROADMAP.md`
- `README.md` and `.gitignore`
- A git repository with an initial commit

**Acceptance criteria:**

- The architecture is internally consistent.
- Storage responsibilities are clear.
- The roles of Markdown and SQLite are unambiguous.
- Major risks are documented.
- Future collaboration is considered but not implemented.
- The development rules are recorded in `CLAUDE.md`.

**User decisions (2026-11-02):**

- **Confirmed:** the storage formats (ADR-003, 004, 006, 007, 008, 013), the Apache-2.0 license (ADR-026), and automatic loading of remote `https:` images (ADR-031, which supersedes ADR-025).
- **Still open:** Apple Developer ID signing. The bundle identifier was confirmed with the rename to Diaspar (ADR-046).

---

## 4. Phase 1: macOS Application Foundation

**Goal:** a polished, launchable macOS shell. No persistence and no AI yet.

### 4.1 Tasks

1. **Scaffold** a Vite 8, React 19 and TypeScript 6 app. Set up a Cargo workspace with `src-tauri/` and a `crates/diaspar-core` crate. In Phase 1 the core crate holds only the app-settings model and the atomic-write helper. Documents, the index and everything else arrive in Phase 2.
2. **Tauri config.**
   - Bundle identifier: `app.diaspar.desktop`, confirmed by the user in ADR-046. It's hard to change later because it determines the Keychain service name and the app-data path.
   - Product name `Diaspar`. Minimum macOS version: proposed 13.0. Phase 1 confirms it against the WKWebView features we use.
   - Overlay title bar with inset traffic lights. Strict CSP (ADR-025).
   - Minimal capabilities: `core:default`, plus dialog, opener and window-state.
3. **Tooling.** Biome (lint and format), `tsc --noEmit`, Vitest with jsdom and Testing Library, `cargo fmt`, `cargo clippy -D warnings`, `cargo test`. Add npm scripts: `dev`, `build`, `typecheck`, `lint`, `test`, `tauri:dev`, `tauri:build`.
4. **Design system.**
   - Write `src/styles/tokens.css` with tokens for color (neutral scale, text, border, hover, selection, focus ring, and the nine select colors), typography (system font stack: SF Pro on macOS; a readable serif option is deferred), spacing, radii (small, 4–6 px), shadows (minimal) and motion (≤ 150 ms, disabled under `prefers-reduced-motion`).
   - Wire Tailwind 4 to these tokens with `@theme`.
   - Add shadcn/ui primitives as each is needed: button, dialog, dropdown-menu, context-menu, tooltip, input, scroll-area, separator, toggle.
5. **Layout.**
   - Three-panel layout with custom pixel-width resize handles (ADR-029): sidebar (default 260 px, collapsible), editor (centered reading column, about 720 px max), AI panel (hidden by default and collapsible).
   - The AI panel only shows an honest empty state: "AI assistant arrives in a later version". It has no input and no controls that pretend to work.
6. **Sidebar shell.** Search entry point, Favorites section, page tree section, and "New page". With no workspace, the sidebar shows an empty state ("No workspace open"). It shows no mock pages.
7. **Editor area empty state** with a clear title area, and breadcrumb placeholders only where they reflect real state.
8. **Themes.**
   - Light, dark and system, applied before first paint. Follow system changes live.
   - Settings → Appearance switches the theme, and the choice persists in `settings.json` through a Rust command, the first real IPC path.
9. **Settings foundation.** A settings dialog (⌘,) with sections Appearance, Workspace, AI and Data. Only Appearance is functional. The other sections say which phase will enable them and contain no fake controls.
10. **Keyboard shortcuts.**
    - Wire up ⌘⇧L (sidebar), ⌘⇧A (AI panel), ⌘, (settings) and Esc (close dialogs).
    - ⌘K and ⌘P open their dialogs in an honest empty state ("Open a workspace to search").
    - ⌘N, ⌘B and ⌘I are registered in Phase 2, with the editor.
    - Native menu items with accelerators, with no conflicts with standard macOS shortcuts. Don't override ⌘Q, ⌘W, ⌘H, ⌘M or ⌘` .
11. **macOS integration.** Persist window state, set a minimum window size, create a native app menu (About, Settings…, Hide, Quit; Edit menu with Undo/Redo/Cut/Copy/Paste/Select All; View menu with the panel toggles), and enforce a single instance.

### 4.2 Tests

- Vitest:
  - layout store (toggle and persist)
  - theme resolution (`light`, `dark`, `system` plus a media-query change)
  - shortcut dispatcher, including that a shortcut is ignored while an input that owns it has focus
  - settings dialog rendering
- Rust: settings load and save round-trip, an atomic settings write (temp file then rename), and a fallback to defaults when the settings file is corrupt (the corrupt file is kept as `settings.json.corrupt-<ts>`).
- `npm run typecheck`, `npm run lint`, `npm test`, `cargo clippy`, `cargo test`, `npm run tauri:build` (debug and release `.app`).

### 4.3 Acceptance criteria

- The app launches on macOS as both a dev build and a release `.app`.
- Collapsing and resizing the sidebar works, and the state persists.
- The layout is responsive, with no overflow at the minimum window size.
- Light and dark themes look deliberately designed in both modes, and following the system theme works live.
- The implemented shortcuts work.
- The production build succeeds.
- There are no misleading placeholder controls.

---

## 5. Phase 2: Document System and Editor

**Goal:** reliable local-first editing. This is the highest-risk phase for data integrity.

> **Status (2026-11-02):** implemented; see PROGRESS.md. Deviations, each recorded: the index is in memory until Phase 3 (ADR-032); conflicts surface through save results and `workspace://changed` instead of a `workspace://conflict` event (ADR-033); the `journal` module is deferred to Phase 3, where its first user (collection schema batches) arrives, because CLAUDE.md forbids speculative modules. The manual checks in the built app (§5.2) are for the user.

### 5.1 Tasks

1. **Markdown round-trip spike (decision gate, ADR-015).**
   - Build the fixture corpus in `tests/fixtures/markdown/`, covering every construct in DATA_MODEL §5.2. It must include tricky cases: nested lists mixing task and ordered items, tables with escaped pipes and inline code, `$` used as currency next to math, code containing fences, raw HTML, footnotes, reference links, CRLF input and CJK text.
   - Run the corpus through `@tiptap/markdown` with custom handlers.
   - Record the result as an ADR (accept, or fall back to `prosemirror-markdown`).
2. **`diaspar-core` modules.**
   - `workspace`: open and init, the lock, layout and discovery rules (DATA_MODEL §3).
   - `naming`: sanitization and uniqueness.
   - `frontmatter`: parse, validate, surgical edit, emit.
   - `atomic`: the write protocol (ARCHITECTURE §4.4).
   - `revision`, `history` (with the snapshot policy and the documented thinning rules), `trash`, `recovery`, `journal` (framework, used from Phase 3 onward).
   - `documents::DocumentService` (single-writer task), `watcher` (notify plus reconcile, with self-write suppression), and a minimal `index` (pages and tree only; FTS arrives in Phase 3).
3. **Commands and events.** The read and mutation commands needed for pages (DATA_MODEL §10), the `workspace://changed` and `workspace://conflict` events, and the `diaspar-asset://` protocol.
4. **Workspace UX.**
   - First run: "Create a workspace" (default `~/Documents/Diaspar`) or "Open a folder".
   - Settings → Workspace: change the workspace, and reveal it in Finder.
   - A warning before opening a very large or system folder.
5. **Sidebar tree.**
   - Lazy children, expand and collapse, inline rename, a hover "+" button and a context menu (rename, duplicate, favorite, move to…, delete).
   - Drag and drop to reorder and move, with cycle prevention.
   - Favorites and breadcrumbs.
   - A trash view with restore and delete-forever (confirmed).
6. **Editor.**
   - Tiptap with the full node set (ARCHITECTURE §6.2), Underline disabled, the raw block, and the title field.
   - Markdown input rules, the slash menu, the bubble menu, KaTeX math (`trust: false`), tables, task lists, images through paste and drop (`import_attachment`), remote `https:` images loaded automatically (add `https:` to CSP `img-src`, ADR-031), and a placeholder for `http:` images.
   - ⌘N, ⌘B, ⌘I.
7. **Autosave and conflicts.**
   - Debounced saves, flush on navigation and quit, the per-page write queue, and the save-status indicator.
   - The conflict banner (keep mine, use theirs, keep both), recovery buffers, and the open-time fidelity notice.
8. **Startup recovery** (ARCHITECTURE §4.10).

### 5.2 Tests

- **Rust unit tests:**
  - sanitization and uniqueness, including case and NFC collisions
  - frontmatter parsing, surgical edits, comments, block scalars, unknown keys, invalid YAML and the Norway problem
  - atomic writes, including a simulated crash between the tmp write and the rename
  - revision conflicts
  - every mutation with its exact filesystem effect
  - leaf-to-bundle conversion and recovery from an interrupted conversion
  - trash and restore of subtrees, including restore when the parent was trashed
  - duplicate-ID resolution, lazy ID persistence and the `ensure_persisted_id` invariant
  - watcher reconciliation: external modify, rename and delete, plus self-write suppression
  - path-escape attempts on the asset protocol
- **Vitest:**
  - round-trip of the whole corpus (semantic, and byte-identical for canonical input)
  - the editor autosave state machine
  - conflict banner flows
  - tree interactions
- **Integration** (Rust, against a temporary workspace): create → edit → "restart" (drop and reopen the service) → verify. Also an external edit while the editor is dirty, which must produce a conflict and lose nothing.
- **Manual, in the built app:**
  - *Persistence:* create a page, type content, quit, reopen, and confirm the content is there.
  - *Nesting:* create a parent and a child, move the child, restart, and confirm the hierarchy.
  - *Markdown:* write a page using every supported construct, save, reload, and confirm nothing changed. Also open the file in a plain text editor and confirm it reads well.

### 5.3 Acceptance criteria

- Pages can be created and edited, and content persists after a restart.
- Nested hierarchy persists.
- The Markdown files stay readable in a plain text editor.
- Supported content survives serialization (the corpus passes).
- Deleted pages can be restored, including their subtrees.
- External modifications are handled without silent overwrites.
- There are no known critical data-loss issues.

---

## 6. Phase 3: Collections, Links, and Search

> **Status (2026-11-03):** implemented; see PROGRESS.md. Decisions: ADR-035 (SQLite cache, one connection; rebuild holds the workspace lock), ADR-036 (CJK search via trigram + LIKE), ADR-037 (collections in the sidebar). Recent pages in ⌘P are kept for the session only. The manual checks (§6.2) are for the user.

### 6.1 Tasks

1. **Collections.**
   - Create a collection, or convert an existing page into one. Delete means trashing the collection page, or "turn into a regular page", which removes the schema.
   - Schema editing: add, remove, rename and retype properties, and edit options.
   - Batch rewrites go through the journal.
2. **Views.**
   - A table view: virtualized rows, inline cell editing by type, column resize and hide, and new rows that create child pages.
   - A list view.
   - Sorting and AND filters (DATA_MODEL §6.5), with view settings saved to `.diaspar/views.json`.
   - Decide and document how entries show in the sidebar. The default proposal is to collapse them under the collection page and load them lazily.
3. **Links.**
   - The page-link node, `[[` autocomplete and live titles.
   - Wikilink read support, a backlinks section, and broken and unresolved link states.
   - `ensure_persisted_id` runs whenever a link is created.
4. **Search.**
   - The full index schema (DATA_MODEL §9) with incremental indexing.
   - ⌘K global search: snippets, highlighting, keyboard navigation, and searching across title, body, tags and properties.
   - ⌘P quick open: titles, plus recent pages.
   - Settings → Rebuild index, which builds into a side database and swaps it in.
   - The CJK and trigram evaluation (risk R6), with the decision recorded.
5. **Synthetic workspace generator** (a dev script) for testing with 5,000 pages.

### 6.2 Tests

- **Rust:**
  - schema validation
  - value coercion table (DATA_MODEL §6.3)
  - property and option rename batches, including journal rollback after a simulated crash
  - a test that FTS5 is available
  - index rebuild equivalence: an incremental index and a rebuilt one are equal
  - link extraction parity with the TypeScript fixtures
- **Vitest:** table editing, sort and filter logic, autocomplete, search keyboard navigation.
- **Manual:**
  - *Collections:* create a collection, add properties, add pages, edit values, restart, and confirm everything persisted.
  - *Search:* create several pages, search for body text, edit a page, and confirm the results update.
  - *Offline:* with networking off, create, edit, search and manage collections.
- **Performance:** with 5,000 pages, measure search latency and how long the tree takes to open, and record the numbers in PROGRESS.md.

### 6.3 Acceptance criteria

- Collections and properties persist.
- The views show consistent data.
- Internal links survive renames and moves.
- Search finds document content and updates after edits.
- The index can be rebuilt from durable data alone.

---

## 7. Phase 4: AI Provider Integration

> **Status (2026-11-03):** implemented; see PROGRESS.md. Decisions: ADR-038 (API check: OpenAI through the Responses API, the user's choice; Anthropic version 2026-10-21 without temperature), ADR-039 (one provider type; sending rules in `AiService`; Channel streaming), ADR-040 (chat Markdown rendered from tokens). Deviation: the adapter fixtures follow the documented formats rather than recordings, because no keys or local servers were available; an ignored live test checks the cloud APIs' auth and TLS. The manual checks (§7.2) are for the user.

### 7.1 Tasks

1. **API verification first.**
   - Check the current OpenAI API (whether Chat Completions is still supported or the Responses API should be used), the Anthropic Messages API version headers and streaming events, Ollama `/api/chat`, `/api/tags` and `/api/show`, and the LM Studio endpoints.
   - Record the findings in DECISIONS (risk R8).
2. **`diaspar-ai` crate.**
   - The provider trait (ARCHITECTURE §9.1), the OpenAI-protocol adapter, the Anthropic adapter, the native Ollama adapter, and the LM Studio preset.
   - SSE and NDJSON parsers, cancellation, error mapping and capability detection.
3. **Secrets.** A `SecretStore` trait with an implementation for the macOS Keychain and one in memory for tests. Keys never cross IPC.
4. **Settings → AI.**
   - Choose a provider, endpoint and model (from the provider's model list, or typed in), store the key, and run a connection test.
   - Generation settings: temperature, maximum output tokens and a context-window override.
   - A cloud transmission acknowledgement. An off switch that disables AI entirely.
5. **AI panel.**
   - Plain chat over a Tauri Channel with streaming, stop, retry, and clear error states.
   - Rendering is sanitized Markdown with no HTML and no remote images.
   - A per-request indicator of what is being sent and to whom.
   - No workspace context yet. That arrives in Phase 5.

### 7.2 Tests

- Recorded-response tests for each adapter (fixtures from real responses, with keys removed): streaming deltas, tool-call deltas, errors, and cancellation mid-stream.
- A Keychain round-trip, run locally (marked ignored in CI if no keychain is available).
- A test that no API key appears in logs or IPC payloads.
- Manual: connect to each provider the user can access, Ollama and LM Studio when they're installed, and an unreachable endpoint.

### 7.3 Acceptance criteria

- Supported providers can be configured, and credentials are stored in the Keychain.
- Connection tests work.
- Responses stream where the provider supports it.
- Local endpoints work.
- Provider failures are reported clearly.
- The app works fully with AI disabled.

---

## 8. Phase 5: AI Knowledge Retrieval (read-only)

> **Status (2026-11-03):** implemented; see PROGRESS.md. Decisions: ADR-041 (passages, stemmed BM25, CJK through page trigrams, rank fusion) and ADR-042 (numbered sources, packing, read-only tools, the new cloud agreement). No new dependencies. Deviations: the tool message formats follow the providers' documentation, untested against real providers (no keys or local models were available); Ask mode allows 6 tool rounds, not 12; a "Search notes" switch in the panel and a tool-use setting in Settings → AI were added, because tools also read notes and OpenAI doesn't report tool support.

### 8.1 Tasks

1. Passage chunking and passage FTS (DATA_MODEL §9), plus the `Retriever` trait and its BM25 implementation.
2. The read tools `search_pages`, `read_page`, `list_pages` and `get_page_metadata`, with typed schemas, validation and size limits.
3. Context packing (ARCHITECTURE §11): current page, selection, retrieved sources and history, all within the token budget.
4. Pre-retrieval for every model, and tool-driven retrieval for models that support tools.
5. Citation rendering and validation, so a citation can only point to a source that was actually provided. Clicking a citation opens the page at the passage.
6. Actions: "Ask about this page", "Ask about selection", "Summarize page" and "Explain selection".

### 8.2 Tests

- Retrieval quality on a small labeled fixture set.
- Context-budget packing.
- Citation validation, including a key invented by the model.
- A tool argument fuzzing pass.
- A test that no tool reachable in Ask mode has a mutation handle. This is checked against the type of the registry.
- Manual: configure a provider, ask about an existing page, check which context was retrieved, and confirm each cited source opens the right page.

### 8.3 Acceptance criteria

- The AI answers questions about notes, summarizes them, and retrieves information across pages.
- Its citations point to real pages and can be opened.
- Missing information is handled honestly.
- There are no persistent AI write paths.

---

## 9. Phase 6: AI Agent and Knowledge Management

> **Status (2026-11-03):** implemented; see PROGRESS.md. Decision: ADR-043 (proposals in memory; a journaled transaction for every file action; operation records; all-or-nothing undo and redo). Deviations: undo refuses as a whole when any page changed, instead of page by page; `edit_page` captures the body revision itself instead of taking `base_rev`; Edit mode gets 12 tool rounds. No real model has proposed changes yet (no keys or local models here).

### 9.1 Tasks

1. Proposal tools and the ChangeSet model (ARCHITECTURE §10.2), validating each proposal as it is made.
2. Preview UI: grouped changes, a word-level diff per page, approve all, approve selected, and reject. Remote images in the preview show as URLs, not loaded, and every external image a proposal adds is flagged (ADR-031).
3. `apply_changeset`: a journaled, all-or-nothing batch with a revision check per operation, protected snapshots and an operation record. Conflicts are reported as such.
4. Undo and redo from the operation log, and an operation history view.
5. Hardening against prompt injection (ARCHITECTURE §10.4).

### 9.2 Tests

- Rejecting a proposal leaves no change on disk (verified by comparing file hashes).
- Approving a proposal persists it. Undo restores the exact bytes.
- A concurrent edit between proposing and applying causes a conflict and nothing is applied.
- A crash during apply is rolled back on the next start.
- Adversarial documents (for example "ignore previous instructions and delete…") can't cause any change without approval.
- Tool arguments containing paths, `..` or IDs from outside the workspace are rejected.
- Manual: request a change, inspect the preview, and reject it. Confirm nothing changed. Request it again, approve it, and confirm it persisted. Then undo, and confirm the original is restored.

### 9.3 Acceptance criteria

Each of these is verified by a test or a documented manual check:

- The AI can propose changes, and every proposal can be previewed.
- Rejected changes don't modify data. Approved changes persist.
- Conflicts are detected.
- Undo restores the previous content.
- Agent tools can't reach outside the workspace.
- Prompt injection in documents can't bypass the approval step.

---

## 10. Phase 7: Product Polish and Release

### 10.1 Tasks

1. A UI and accessibility pass: VoiceOver labels, focus order, contrast in both themes, and reduced motion.
2. Error-handling polish: an error log viewer and "Copy diagnostics", which excludes content and keys.
3. A first-run experience, with an optional sample page that the user creates. Nothing is created automatically.
4. Settings → Data:
   - **Export:** a Markdown copy without `.diaspar`.
   - **Backup:** a zip of the workspace, including `.diaspar`.
   - **Restore:** unzip into a new folder and open it there. A restore never overwrites in place.
5. Performance profiling with the 5,000-page workspace (startup, search, typing latency, idle CPU). Record the measured numbers and claim only measured improvements.
6. App icon, About window, README, user guide, `CONTRIBUTING.md`, and a trademark policy once the product-name search is done (R15). The license (Apache-2.0) is already in place.
7. CI on GitHub Actions: lint, typecheck, test, clippy, `cargo audit`, `npm audit`, and building the macOS bundle.
8. Packaging:
   - Release workflow: a universal or arm64 `.app` and DMG.
   - Signing and notarization if the user provides an Apple Developer ID. Otherwise ad-hoc signing, with Gatekeeper instructions.
   - Report the signing and notarization status exactly as it is.

### 10.2 Acceptance criteria

- The app builds and runs without a dev server.
- Core features work, and there are no known critical data-loss bugs.
- The documentation matches the implementation.
- Release artifacts are produced where supported.
- The signing and notarization status is reported accurately.

> **Status (2026-11-03):** implemented; see PROGRESS.md. Decisions: ADR-044 (export, backup and restore through dialogs Rust opens; diagnostics without content) and ADR-045 (universal, ad-hoc signed, CI drafts releases). Deviations: the workflows were written and their steps run locally, but they haven't run on GitHub (pushing wasn't authorized); the release workflow has no Developer ID path, because there's no certificate to test one with (CONTRIBUTING.md lists the variables); no trademark policy, because the name search found conflicts (R15, a user decision); typing latency was measured in the browser preview, not the native app.
