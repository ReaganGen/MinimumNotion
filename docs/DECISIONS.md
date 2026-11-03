# Architectural Decision Records

Each record lists its Status, Date, Context, Decision, Alternatives, Rationale and Consequences. Status is one of **Accepted**, **Proposed** (awaiting the user's approval) or **Superseded by ADR-N**. Don't edit an accepted decision in place. Write a new ADR that supersedes it.

| ADR | Title | Status |
|---|---|---|
| 001 | Tauri 2 + Rust backend + React/TypeScript frontend | Accepted |
| 002 | Markdown files are the source of truth; SQLite is a derived cache | Accepted |
| 003 | The index database lives outside the workspace | Accepted, user-confirmed |
| 004 | The directory tree mirrors the page tree (leaf file / bundle folder with `index.md`) | Accepted, user-confirmed |
| 005 | Page identity is a UUID in frontmatter, persisted lazily | Accepted |
| 006 | The title is authoritative in frontmatter; the filename is derived | Accepted, user-confirmed |
| 007 | Page links are standard Markdown links to `forma://page/<uuid>` | Accepted, user-confirmed |
| 008 | Attachments are co-located in the page's `_assets/` folder | Accepted, user-confirmed |
| 009 | A revision is a content hash; optimistic concurrency on every write | Accepted |
| 010 | History is full-file snapshots in `.forma/history/` | Accepted |
| 011 | Rust owns the file format; TypeScript owns body Markdown ⇄ editor | Accepted |
| 012 | Surgical frontmatter editing preserves unknown content | Accepted |
| 013 | A collection is a page; schema in its frontmatter; entries are its children | Accepted, user-confirmed |
| 014 | Sibling order and favorites live in `.forma/tree.json` (advisory) | Accepted |
| 015 | Editor: Tiptap 3 + `@tiptap/markdown`, gated by a round-trip spike | Accepted (gated) |
| 016 | TypeScript 6.0 with Biome, not TypeScript 7 or ESLint | Accepted |
| 017 | Rust workspace: `forma-core`, `forma-ai`, thin `src-tauri` shell | Accepted |
| 018 | All AI traffic goes through Rust; the webview has no network access | Accepted |
| 019 | Provider abstraction: two wire protocols plus presets, with capability detection | Accepted; the OpenAI preset changed by ADR-038 |
| 020 | Credentials in the macOS Keychain via `keyring-core` | Accepted |
| 021 | The agent can only propose; the user applies; operations are logged and undoable | Accepted |
| 022 | Retrieval uses FTS5 BM25 passages behind a `Retriever` trait; no embeddings in v0.1 | Accepted |
| 023 | Typed IPC through `ts-rs`-generated bindings | Accepted |
| 024 | npm and Node 24 LTS | Accepted |
| 025 | No remote content is loaded in the webview | Superseded by ADR-031 |
| 026 | Open-source license: Apache-2.0 | Accepted, user-confirmed |
| 027 | Single instance plus a workspace lock | Accepted |
| 028 | YAML handled by the saphyr family, not the deprecated `serde_yaml` | Accepted |
| 029 | Custom pixel-width resize handles instead of `react-resizable-panels` | Accepted |
| 030 | App shortcuts are native menu accelerators, routed through one command registry | Accepted |
| 031 | Remote `https:` images in notes load automatically; AI output and AI previews still don't | Accepted, user-chosen |
| 032 | The Phase 2 page index lives in memory; SQLite arrives with search | Accepted; extended by ADR-035 |
| 033 | The editor's saves are checked against the body revision | Accepted, user-approved |
| 034 | Outcome of the Markdown gate: `@tiptap/markdown` plus a Forma rules layer | Accepted |
| 035 | The SQLite cache: loaded at open, written with every change, one connection | Accepted |
| 036 | CJK search uses a trigram table with LIKE; words use unicode61 | Accepted |
| 037 | Collections in the sidebar and in pages | Accepted |
| 038 | Provider APIs as verified in Phase 4: OpenAI through the Responses API, Anthropic API version 2026-10-21 | Accepted, user-chosen |
| 039 | The AI service: one provider type, rules in Rust, answers over a Channel | Accepted |
| 040 | Chat answers are rendered from Markdown tokens as React elements | Accepted |
| 041 | Retrieval as built: passage chunks, stemmed BM25, CJK through page trigrams, rank fusion | Accepted |
| 042 | Asking about notes: numbered sources, a packed context, read-only tools, and a new cloud agreement | Accepted |

---

## ADR-001: Tauri 2 + Rust backend + React/TypeScript frontend

- **Status:** Accepted. **Date:** 2026-11-02
- **Context:** The spec prefers Tauri 2, Rust, React, TypeScript and Vite. We checked whether anything is significantly better for a local-first macOS app.
- **Decision:** Use the preferred stack, pinned to current stable versions (IMPLEMENTATION_PLAN §2): Tauri 2.12, React 19.3 and Vite 8.3. Tauri 3 is in alpha and is not used.
- **Alternatives:**
  - **Electron.** Mature, but the app bundle is typically ten or more times larger, idle memory is higher, and its security posture is weaker by default.
  - **Native Swift/AppKit.** The best platform feel, but there's no mature ProseMirror-class rich-text editor, and it would mean rewriting everything for a future cross-platform build.
  - **Pure web.** Out of scope.
- **Rationale:** Small bundle, a Rust backend suited to data integrity work (atomic I/O, SQLite, hashing), a capability-based IPC security model, and ProseMirror for the editor.
- **Consequences:** We depend on WKWebView behavior (IME, drag-and-drop, clipboard). Native-feel details like menus, the title bar and shortcuts need deliberate work.

## ADR-002: Markdown files are the source of truth; SQLite is a derived cache

- **Status:** Accepted. **Date:** 2026-11-02
- **Context:** The spec requires portable, human-readable data, and requires that deleting the index loses nothing.
- **Decision:** Every user-authored datum lives in Markdown files, YAML frontmatter or `.forma/*.json`. SQLite holds only data that can be rebuilt from those files.
- **Alternatives:** SQLite as primary storage with a Markdown export (simpler queries, but it breaks portability and external editing). A dual-write design where both are authoritative (consistency bugs).
- **Rationale:** User ownership, durability, and simple recovery: the index can always be rebuilt.
- **Consequences:** Queries depend on index freshness, so reconciliation must be robust. Collection queries can't rely on SQLite constraints for integrity, which means validation happens in the Document Service.

## ADR-003: The index database lives outside the workspace

- **Status:** Accepted, confirmed by the user on 2026-11-02. **Date:** 2026-11-02
- **Context:** The spec's illustrative layout places `index.db` in `.forma/`. Workspaces are often stored in Dropbox, iCloud Drive or git.
- **Decision:** Store the index at `~/Library/Application Support/<bundle-id>/index/<workspace-key>/index.db`.
- **Alternatives:** `.forma/index.db`, as illustrated in the spec. Sync tools corrupt SQLite databases or create conflict copies of them, and the file bloats git repositories and backups.
- **Rationale:** It is a cache, and machine-local caches don't belong in synced data. Integrity is protected because the index can always be rebuilt.
- **Consequences:** Moving the workspace folder triggers a rebuild, which is cheap. "Rebuild index" and "Reveal index" must handle the external location.

## ADR-004: The directory tree mirrors the page tree

- **Status:** Accepted, confirmed by the user on 2026-11-02. **Date:** 2026-11-02
- **Context:** Pages nest to arbitrary depth. Hierarchy has to be durable, portable and visible outside Forma, and multi-file operations are a source of corruption.
- **Decision:** A leaf page is `Name.md`. A page with children or attachments is a bundle, `Name/index.md`, and its children live inside `Name/`. A page's parent is derived from its location. See DATA_MODEL §3.
- **Alternatives:**
  1. **A flat `pages/` folder with `parent:` in frontmatter.** The simplest code, but the hierarchy is invisible in Finder and other editors, which is a real portability loss.
  2. **`Name.md` with a sibling `Name/` folder for children** (the Notion-export style). A move or rename touches two entries, and the two can't be renamed atomically together.
  3. **Every page as a folder.** Uniform, but it clutters Finder and other editors with an `index.md` for every leaf.
- **Rationale:** Moves, renames, trash and restore each become one atomic `rename(2)`, with no journal needed. The hierarchy is readable in any file browser. `index.md` page bundles are an established convention (Hugo, MkDocs, Docusaurus).
- **Consequences:** Filename sanitization and case- and normalization-insensitive uniqueness are required. A leaf has to be converted to a bundle when it gets its first child or attachment. Relative references to files *outside* a page's own `_assets/`, which users can write by hand, may break when the page moves. That is documented as a limitation, and the conversion step rewrites those references with a `../` prefix. Tools that resolve wikilinks by filename (for example Obsidian) see bundle pages named "index".

## ADR-005: Page identity is a UUID in frontmatter, persisted lazily

- **Status:** Accepted. **Date:** 2026-11-02
- **Context:** Identity must not depend on title or path. Opening a folder of existing Markdown must not mass-rewrite the user's files.
- **Decision:** Use a UUID v4 stored as frontmatter `id`. Pages discovered without an ID get an index-only ID, which is persisted only when Forma writes the file or just before something durable references the page (`ensure_persisted_id`). Duplicate IDs are resolved deterministically (DATA_MODEL §4.4).
- **Alternatives:**
  - Write IDs into every file on discovery. This silently modifies the user's files in bulk.
  - Keep IDs only in the index. They would be lost on rebuild.
  - Use path as identity. It breaks on move and rename.
- **Rationale:** Stable identity without surprising writes.
- **Consequences:** Two ID states to handle carefully, and the invariant needs dedicated tests (risk R12). Files copied in Finder are detected and given new IDs.

## ADR-006: The title is authoritative in frontmatter; the filename is derived

- **Status:** Accepted, confirmed by the user on 2026-11-02. **Date:** 2026-11-02
- **Context:** The spec's example has the title both in `title:` and as an `# H1`. The spec also says to avoid duplicated authoritative state. Filenames can't represent every title, because of `/`, `:`, length limits and collisions.
- **Decision:** `title` in frontmatter is authoritative. When it is missing, the filename stem is used. The body doesn't repeat the title as an H1, because the editor shows the title field as the page heading. Forma renames the file when the title changes in Forma. It doesn't rename the file back if the user renames it in Finder.
- **Alternatives:**
  - Filename as the title (Obsidian style). Lossy for characters a filename can't hold, and collisions need visible suffixes.
  - The H1 as the title. Ambiguous, and it mixes metadata with content.
- **Rationale:** Exact titles and a single authority, while filenames stay human-friendly.
- **Consequences:** Renaming a file in Finder doesn't change the title shown in Forma. This is documented. One thing to consider later is omitting `title:` when the filename represents it losslessly.

## ADR-007: Page links are standard Markdown links to `forma://page/<uuid>`

- **Status:** Accepted, confirmed by the user on 2026-11-02. **Date:** 2026-11-02
- **Context:** The spec asks for `[[Page Name]]` syntax, resolution by stable ID, backlinks, and safe renaming. Titles are not unique: two "Notes" pages can exist under different parents.
- **Decision:** On disk, a page link is `[Current Title](forma://page/<uuid>)`. Typing `[[` opens title autocomplete and inserts that link. Literal `[[Wikilinks]]` written by other tools are resolved by title, read-only, and kept exactly as written. When the source page is saved, its link labels are refreshed to the targets' current titles.
- **Alternatives:**
  1. **`[[Title]]` on disk, with every referencing file rewritten on rename** (the Obsidian approach). The most familiar syntax, but titles are ambiguous, a rename cascades into writes to many files (more conflict surface), and a link breaks if the target is renamed outside the app.
  2. **`[[<uuid>|Title]]`.** ID-stable, but noisy, and it isn't standard Markdown.
  3. **Relative path links (`../Notes.md`).** Clickable on GitHub, but they break on move and need cascading rewrites.
- **Rationale:** Correctness and integrity first: links are unambiguous, they survive renames and moves, and they never require rewriting other files. The syntax is standard CommonMark, so both parsers handle it without custom extensions, and every Markdown tool displays it as readable link text.
- **Consequences:** Outside Forma, the link target (`forma://…`) isn't clickable unless Forma later registers the URL scheme for deep links, which would be a natural follow-up. Custom link labels for page links aren't supported in v0.1, because labels track the title.

## ADR-008: Attachments are co-located in the page's `_assets/` folder

- **Status:** Accepted, confirmed by the user on 2026-11-02. **Date:** 2026-11-02
- **Context:** The spec's illustration has a workspace-level `assets/`. Relative references from nested pages to a root `assets/` folder depend on the page's depth and break whenever the page moves.
- **Decision:** A page's attachments live in `<bundle>/_assets/` and are referenced as `_assets/<name>`, relative to `index.md`.
- **Alternatives:** A workspace-level `assets/` folder, which needs reference rewrites in the page and all its descendants on every move. Root-relative paths (`/assets/x.png`), which aren't portable.
- **Rationale:** References never change on rename or move, and attachments travel with the page to the trash and back, or when it is duplicated.
- **Consequences:** A page that has attachments is always a bundle. A file can't be shared between pages without copying it, which is acceptable for v0.1.

## ADR-009: A revision is a content hash; optimistic concurrency on every write

- **Status:** Accepted; refined for the editor by ADR-033. **Date:** 2026-11-02
- **Context:** Revisions are needed for conflict detection between the editor, external edits and AI proposals.
- **Decision:** `rev` is the first 16 hex characters of the BLAKE3 hash of the file bytes. Every mutation of an existing page carries `base_rev` and is rejected if the file has changed.
- **Alternatives:**
  - A revision counter in frontmatter. External tools don't increment it, it adds noise to every file, and it is a second source of truth.
  - File mtime. Unreliable across copies, sync tools and git.
- **Rationale:** It detects every change automatically, including external ones, and stores nothing extra.
- **Consequences:** Each check re-reads and re-hashes the file, which is cheap at Markdown sizes. There is a residual time-of-check-to-time-of-use window, documented in ARCHITECTURE §4.5.

## ADR-010: History is full-file snapshots in `.forma/history/`

- **Status:** Accepted. **Date:** 2026-11-02
- **Context:** AI undo, conflict resolution, restoring revisions and recovering from external overwrites all need old versions.
- **Decision:** Store plain full copies of the file bytes, with coalescing and protected-snapshot rules (ARCHITECTURE §4.6).
- **Alternatives:**
  - Diffs or deltas. Smaller, but fragile (a corrupt base breaks the chain) and not human-recoverable.
  - Git under the hood. Heavy, and surprising when the user already uses git in that folder.
  - SQLite blobs. That would put user data in the cache.
- **Rationale:** Robust, simple, readable without Forma, and Markdown files are small.
- **Consequences:** Disk use grows with editing. A thinning policy is required, specified in Phase 2.

## ADR-011: Rust owns the file format; TypeScript owns body Markdown ⇄ editor

- **Status:** Accepted. **Date:** 2026-11-02
- **Context:** The editor (ProseMirror) runs in TypeScript. Persistence, indexing, the watcher and AI tools run in Rust.
- **Decision:** Rust assembles files and parses and edits frontmatter. TypeScript converts the body between Markdown and ProseMirror. Rust extracts text, links and attachments from the body with `pulldown-cmark` for indexing. A shared fixture corpus keeps the two sides in agreement.
- **Alternatives:**
  - All Markdown handling in TypeScript. Index rebuilds and external-change handling would then need the webview running, which couples persistence to the UI.
  - All of it in Rust, with Tiptap fed JSON. That means reimplementing the ProseMirror schema mapping in Rust.
- **Rationale:** Each side does what its ecosystem does best, and persistence never depends on the UI.
- **Consequences:** There are two Markdown parsers. The Rust parser only extracts data, which limits how much a disagreement can matter, and fixtures catch drift.

## ADR-012: Surgical frontmatter editing preserves unknown content

- **Status:** Accepted. **Date:** 2026-11-02
- **Context:** Users and other tools put their own keys, comments and formatting in frontmatter. Serializing the whole block again would drop comments and reorder keys.
- **Decision:** Edit only the byte ranges of the top-level keys that change. Leave everything else untouched. Fall back to metadata-read-only when safe span edits aren't possible (DATA_MODEL §4.6).
- **Alternatives:**
  - Parse into a value tree and serialize it all again. Simple, but lossy for comments and style.
  - A round-trip-preserving YAML library. None in the Rust ecosystem is mature.
- **Rationale:** "Never destroy what you don't understand."
- **Consequences:** Needs a span-aware parser and careful tests of block scalars, comments and multi-line values.

## ADR-013: A collection is a page; schema in its frontmatter; entries are its children

- **Status:** Accepted, confirmed by the user on 2026-11-02. **Date:** 2026-11-02
- **Context:** The spec describes collections as simplified Notion databases, where a page belongs to at most one primary collection, and the illustrative layout shows `collections/*.json`.
- **Decision:** A collection is a bundle page whose frontmatter contains a `collection:` schema. Its direct children are its entries. Entry values are top-level frontmatter keys named after the properties. View settings (sort, filter, hidden columns) are kept in `.forma/views.json`, which is advisory. See DATA_MODEL §6.
- **Alternatives:**
  - `collections/<id>.json` with an explicit `collection:` membership key on each page. Membership would be independent of the tree, but there'd be a second source of truth, and orphaned schemas or members.
  - Values nested under a `properties:` key. Less likely to clash with other keys, but most frontmatter tools only read top-level keys.
  - Values keyed by property ID. Renames would be free, but the files would be unreadable.
- **Rationale:** It matches the Notion mental model (database rows are pages inside the database). Membership can't be inconsistent with location. The schema travels with the collection when it moves. Entry files stay readable.
- **Consequences:** Renaming a property or an option rewrites every entry in a journaled batch, which is rare. Removing a property is non-destructive. View settings are lost if `.forma/` is lost, which loses no data. Multi-collection membership is deferred, and the format can grow to support it.

## ADR-014: Sibling order and favorites live in `.forma/tree.json` (advisory)

- **Status:** Accepted. **Date:** 2026-11-02
- **Context:** Manual sibling order and favorites need persisting. Putting them in frontmatter would rewrite user files for what are UI preferences.
- **Decision:** Store an ordered list of child IDs per parent, plus a favorites list, in `.forma/tree.json`. Missing entries fall back to natural title order.
- **Alternatives:** An `order:` fractional key in each page's frontmatter (noise in every file, and every reorder writes user files). A per-folder hidden order file (pollutes the user's folders).
- **Rationale:** User files stay clean, reorder is one small atomic write, and losing the file costs only cosmetic state.
- **Consequences:** Pages must have persisted IDs before they can be ordered or favorited (ADR-005). Reordering in Finder isn't possible, which is expected anyway.

## ADR-015: Editor: Tiptap 3 + `@tiptap/markdown`, gated by a round-trip spike

- **Status:** Accepted. The gate passed; the outcome is ADR-034. **Date:** 2026-11-02
- **Context:** Tiptap 3.32 ships an official `@tiptap/markdown` extension, based on `marked` 17. Round-trip fidelity is a hard requirement.
- **Decision:** Start Phase 2 with a spike that runs the fixture corpus through `@tiptap/markdown`, including custom handlers for math, page links, wikilinks and raw blocks. Adopt it only if it passes. Otherwise, use a custom `prosemirror-markdown` serializer with `markdown-it`. Use only MIT-licensed Tiptap packages, with no Tiptap Pro or cloud extensions.
- **Alternatives:** The community `tiptap-markdown` package (last released September 2025, so maintenance is uncertain). Lexical or Slate (the Markdown and table support is less mature, and the spec prefers Tiptap).
- **Rationale:** Use the official, maintained path where possible, with a known-good fallback.
- **Consequences:** Phase 2 has a decision gate that must be recorded here as a superseding ADR if the fallback is chosen. `@tiptap/extension-mathematics` currently accepts KaTeX ≤ 0.18, so KaTeX is pinned to 0.18.x instead of the latest 0.19.

## ADR-016: TypeScript 6.0 with Biome, not TypeScript 7 or ESLint

- **Status:** Accepted. **Date:** 2026-11-02
- **Context:** TypeScript 7.0, the native Go port, is now stable. `typescript-eslint` 8.72, however, supports only `typescript >=4.8.4 <6.1.0`.
- **Decision:**
  - TypeScript **6.0.x** for type checking.
  - **Biome 2.6** for linting and formatting, with no ESLint.
  - Vite and Vitest transpile with their own toolchain, so the TypeScript version affects only type checking.
- **Alternatives:**
  - TypeScript 7 without ESLint. This is faster, but other tools that rely on the TypeScript API may not support it yet.
  - TypeScript 6 with ESLint and typescript-eslint. This means more dependencies and slower runs.
- **Rationale:** It uses the stable, fully compatible toolchain and keeps the dependency count low.
- **Consequences:** Revisit TypeScript 7 once its ecosystem catches up. Biome's type-aware rules are a subset of what typescript-eslint offers, and that is acceptable.

## ADR-017: Rust workspace: `forma-core`, `forma-ai`, thin `src-tauri` shell

- **Status:** Accepted. **Date:** 2026-11-02
- **Decision:**
  - `crates/forma-core` holds the workspace, documents, frontmatter, index, watcher and collections. It has no Tauri and no network dependencies.
  - `crates/forma-ai` holds the providers, agent, tools and ChangeSets.
  - `src-tauri` holds only commands, events, window and menus, and packaging.
- **Alternatives:** A single Tauri crate. Simpler at first, but every test has to build WebKit bindings, and the network boundary isn't visible.
- **Rationale:** A fast test loop, core logic that isn't tied to a platform (one of the spec's goals), and a security boundary that shows up in the dependency graph.
- **Consequences:** Some boilerplate for error and type conversion at the crate boundaries.

## ADR-018: All AI traffic goes through Rust; the webview has no network access

- **Status:** Accepted. **Date:** 2026-11-02
- **Decision:**
  - Provider HTTP clients (`reqwest`), streaming and the agent loop run in Rust.
  - Responses stream to the UI over Tauri Channels.
  - The CSP blocks every network origin from the webview.
- **Alternatives:** Calling providers from JavaScript with their official SDKs. The code is shorter, but API keys would sit in webview memory and the webview would need network access.
- **Rationale:** Keys never enter JavaScript. A compromised renderer can't exfiltrate data directly. Tool execution happens next to the Document Service, behind the authorization boundary.
- **Consequences:** We write our own SSE and NDJSON parsing and our own adapters instead of using vendor SDKs, so we need recorded-response tests.

## ADR-019: Provider abstraction: two wire protocols plus presets, with capability detection

- **Status:** Accepted. The OpenAI preset uses the Responses API instead (ADR-038), so there are three wire protocols. **Date:** 2026-11-02
- **Decision:**
  - Two adapters: the OpenAI Chat Completions protocol (covering OpenAI, OpenAI-compatible endpoints and LM Studio) and the Anthropic Messages protocol.
  - A native Ollama adapter, needed for `num_ctx` and for capability detection.
  - Model lists come from the providers' APIs.
  - Capabilities are tri-state (`Yes`, `No`, `Unknown`), and a capability marked `Unknown` is treated as missing for anything that depends on it.
- **Alternatives:** A third-party multi-provider library, which would add a large dependency and its own release cadence. A separate adapter per vendor, which would duplicate code.
- **Rationale:** The minimum code that covers all five required targets accurately.
- **Consequences:** Phase 4 must first check whether OpenAI still supports Chat Completions or whether the Responses API should be used for OpenAI itself (risk R8).

## ADR-020: Credentials in the macOS Keychain via `keyring-core`

- **Status:** Accepted. **Date:** 2026-11-02
- **Decision:** Use `keyring-core` 1.x with `apple-native-keyring-store` 1.x, behind a small `SecretStore` trait so an in-memory store can be used in tests.
- **Alternatives:**
  - The `keyring` 4.x umbrella crate. It pulls in stores for other platforms.
  - `security-framework` directly. It is lower-level, and other operating systems are harder to support later.
  - A Tauri stronghold or store plugin. Not the platform's own secure store.
- **Rationale:** It is the platform's secure store, has a small dependency surface and stays portable.
- **Consequences:** Builds that aren't signed consistently trigger Keychain access prompts during development (risk R7).

## ADR-021: The agent can only propose; the user applies; operations are logged and undoable

- **Status:** Accepted. **Date:** 2026-11-02
- **Decision:**
  - The agent runtime holds a read-only Document Service handle and a ChangeSet builder.
  - `apply_changeset` is a user-only IPC command.
  - Applies are journaled, all-or-nothing, and checked against each operation's `base_rev`.
  - Each applied operation is written to `.forma/operations/` with protected snapshots, so undo can restore pages when nothing has changed since.
- **Alternatives:**
  - Letting the model call mutation tools after a confirmation prompt. Authorization would then rest on model compliance and UI timing.
  - Auto-applying changes with undo. That violates the spec.
- **Rationale:** Authorization is enforced by construction, so prompt injection can't bypass it.
- **Consequences:** Act mode needs a model that supports tool calling, and preview UX is substantial work in Phase 6.

## ADR-022: Retrieval uses FTS5 BM25 passages behind a `Retriever` trait; no embeddings in v0.1

- **Status:** Accepted. **Date:** 2026-11-02
- **Decision:**
  - Passage-level FTS5 retrieval, with pre-retrieval for every model.
  - Optional tool-driven search for models that support tools.
  - Citation keys checked against the sources that were actually provided.
- **Alternatives:** Embeddings with a vector store. Better recall for paraphrased queries, but it needs an embedding model (one that is local or one that sends data to a cloud service), plus storage and index maintenance.
- **Rationale:** The spec says no mandatory vector database. FTS works offline with any model.
- **Consequences:** Recall is weaker for semantic queries. The trait leaves room to add hybrid retrieval later.

## ADR-023: Typed IPC through `ts-rs`-generated bindings

- **Status:** Accepted. **Date:** 2026-11-02
- **Decision:** Derive TypeScript types from Rust IPC types with `ts-rs` 12. The generated files are committed, and CI checks that they match. Commands are wrapped by hand in `src/lib/ipc/`.
- **Alternatives:** `specta` and `tauri-specta` 2.x, which generate command bindings too but are still release candidates (`2.0.0-rc.27`). Types maintained by hand, which drift.
- **Rationale:** A stable tool with a single source of truth for types.
- **Consequences:** The command wrapper functions are written by hand. They are thin, and a contract test covers them.

## ADR-024: npm and Node 24 LTS

- **Status:** Accepted. **Date:** 2026-11-02
- **Decision:** Use npm with a committed `package-lock.json`. The minimum Node version is 24 LTS ("Krypton", currently 24.22). Node 26 LTS also works.
- **Alternatives:** pnpm or bun. Faster, but contributors need one more tool.
- **Rationale:** It comes with Node, and Vite 8 (`^20.19 || >=22.12`) and Vitest 5 (`^22.12 || ^24 || >=26`) both support it.
- **Consequences:** None significant.

## ADR-025: No remote content is loaded in the webview

- **Status:** Superseded by ADR-031 (2026-11-02): the user chose to load remote images in notes. **Date:** 2026-11-02
- **Decision:** The CSP allows only `'self'`, the IPC origin and the `forma-asset:` protocol. Remote images in documents and AI output are shown as placeholders and never fetched. External links open in the system browser when the user clicks them.
- **Alternatives:** Allowing remote images, which is convenient but leaks reading activity and enables data exfiltration through prompt injection.
- **Rationale:** Local-first privacy, and closing an exfiltration path for prompt injection.
- **Consequences:** Pages that embed remote images look less complete. A per-image "Load" action could be added later.

## ADR-026: Open-source license

- **Status:** Accepted: **Apache-2.0**, chosen by the user on 2026-11-02. **Date:** 2026-11-02
- **Context:** Forma is meant to be open core, with commercial offerings that are hosted services (sync, collaboration, team administration). The spec requires documenting tradeoffs and getting approval before choosing. Nothing is licensed yet, so no `LICENSE` file has been created.
- **Options:**

| License | Forks may be closed-source? | Effect on contributors and companies | Mac App Store | Fit with an open-core model |
|---|---|---|---|---|
| **Apache-2.0** | Yes | Most welcoming, and it includes an explicit patent grant | Compatible | Value is protected by the brand (trademark) and the hosted services, not by the license |
| **MIT** | Yes | Most welcoming, but no patent grant | Compatible | Same as Apache-2.0 |
| **MPL-2.0** | Only for new files. Changes to Forma's files must stay open | Moderate (copyleft at the file level) | Generally compatible | Improvements to the core stay open, and proprietary add-ons can be separate files |
| **GPL-3.0** | No | Some companies avoid it | Problematic for distribution by anyone else. The copyright holder can still publish under other terms, which needs a CLA once outside contributions arrive | Prevents closed forks of the desktop app |
| **AGPL-3.0** | No, and modifications offered over a network must be published too | Same as GPL, and stricter | Same as GPL | Used by peer note apps (for example AppFlowy and Logseq). Its network clause matters little for a desktop client |
| FSL / BSL (source-available) | — | Not OSI-approved open source | — | Conflicts with the "open source core" goal. Not recommended |

- **Recommendation:** **Apache-2.0** for the core, together with a trademark policy for the name "Forma". Forma plans to make money from hosted services, which are separate code, not from the desktop client, so copyleft on the client adds little protection and costs adoption. Apache-2.0 also keeps future Mac App Store distribution and dual licensing simple without a CLA.
  - **If preventing closed-source forks of the app is the priority, choose AGPL-3.0 instead.** It should come with a CLA from the start, so the project keeps the option to sell commercial licenses.
- **Dependency check:** Every planned dependency is under a permissive license compatible with all of the options above: Tauri (MIT/Apache-2.0), React (MIT), Tiptap core (MIT), ProseMirror (MIT), SQLite (public domain), KaTeX (MIT), shadcn/ui (MIT), Lucide (ISC), rusqlite (MIT).
- **Decision (2026-11-02):** The user chose **Apache-2.0**. The repository now has the standard Apache License 2.0 text in `LICENSE`, and `package.json` and `Cargo.toml` declare `Apache-2.0`. No CLA: under Apache-2.0 §5, contributions are licensed under the same terms by default.
- **Consequences:** `CONTRIBUTING.md` and a trademark policy for the product name are written before the first public release (Phase 7). The trademark policy waits for the name search (risk R15), because "Forma" may collide with existing products.

## ADR-027: Single instance plus a workspace lock

- **Status:** Accepted. **Date:** 2026-11-02
- **Decision:** Use `tauri-plugin-single-instance` and an advisory `flock` on `.forma/lock`. If the lock is held, for example by a second instance or a different build, the workspace opens read-only.
- **Rationale:** Two writers on one workspace break the single-writer guarantee.
- **Consequences:** The lock doesn't help when the same workspace is synced to another machine. That case is handled through revision checks and duplicate-ID handling (risk R3).

## ADR-028: YAML handled by the saphyr family, not the deprecated `serde_yaml`

- **Status:** Accepted. **Date:** 2026-11-02
- **Context:** `serde_yaml` is marked deprecated (0.9.34+deprecated, archived in 2024). Surgical editing (ADR-012) needs a parser that reports source spans.
- **Decision:** Use `saphyr` / `saphyr-parser` (YAML 1.2, maintained) for span-aware parsing, and Forma's own emitter for the values it writes. Phase 2 confirms the exact crate split and whether `serde-saphyr` 2.x is worth using for typed deserialization of `.forma/` files. Those files are JSON, so they use `serde_json`.
- **Alternatives:** `serde_yaml_ng` and `serde_norway` (forks of the deprecated crate with slow maintenance, and no span API). `yaml-rust2` (maintained, but its span support is weaker).
- **Consequences:** We need our own value-emission code (DATA_MODEL §4.7), with tests against YAML 1.1 ambiguities such as the "Norway problem".

## ADR-029: Custom pixel-width resize handles instead of `react-resizable-panels`

- **Status:** Accepted. **Date:** 2026-11-02 (Phase 1)
- **Context:** The plan listed `react-resizable-panels` for the three-panel layout. Forma's side panels are fixed-width columns in pixels, like Notion's sidebar: their width shouldn't change when the window is resized. They also need to collapse smoothly and turn into overlays in narrow windows.
- **Decision:** Use a small `ResizeHandle` component (pointer capture, keyboard arrows, Home/End, double-click to reset, `role="separator"` with ARIA values). Panel widths are stored in pixels and clamped in both TypeScript and Rust.
- **Alternatives:** `react-resizable-panels` 4.x. It's capable, but its model is built around a group of panels that share the space, which is more than two independent fixed-width columns need.
- **Rationale:** About 100 lines of code, with full control over animation and overlay behavior, and one dependency fewer.
- **Consequences:** We own the handle's accessibility and pointer behavior, which tests cover.

## ADR-030: App shortcuts are native menu accelerators, routed through one command registry

- **Status:** Accepted. **Date:** 2026-11-02 (Phase 1)
- **Context:** App shortcuts (⌘⇧L, ⌘⇧A, ⌘K, ⌘P, ⌘,) should show in the macOS menu bar. If the native menu and a JavaScript key handler both reacted to the same key, the shortcut could fire twice, depending on how WebKit orders key equivalents.
- **Decision:** Each app-level shortcut is defined once, as a menu item accelerator in `src-tauri/src/menu.rs`. A menu click or accelerator sends `forma://menu` with a command ID. The frontend runs it through `runCommand` in `src/features/commands/commands.ts`. The webview doesn't listen for these keys. A Rust test checks that every menu ID exists in the frontend registry. Editor-level shortcuts (⌘B, ⌘I) stay in the editor (Phase 2).
- **Alternatives:** JavaScript key handlers alone. The shortcuts wouldn't be visible or discoverable in the menu bar, and they'd conflict with macOS conventions.
- **Consequences:** The display labels in `SHORTCUTS` have to be kept in step with the accelerators in `menu.rs`. A test checks the IDs, not the key labels. In the development browser preview, a harness maps the same keys to menu events.

## ADR-031: Remote `https:` images in notes load automatically

- **Status:** Accepted, chosen by the user on 2026-11-02. Supersedes ADR-025. **Date:** 2026-11-02
- **Context:** ADR-025 blocked all remote content, so web images embedded in notes would only show as placeholders. The user prefers notes to display the way they were written.
- **Decision:**
  1. Images in notes whose URL uses `https:` are loaded automatically. The CSP `img-src` becomes `'self' forma-asset: https:`. This lands in Phase 2, when the editor first renders images; Phase 1 renders none, so its CSP is unchanged.
  2. Plain `http:` images are **not** loaded and show a placeholder with their URL, because unencrypted requests can be read and altered on the network. This detail was chosen by Claude; the user can override it.
  3. Everything else stays blocked: `connect-src` remains IPC-only, and no remote scripts, styles, fonts or frames are allowed. All AI traffic still goes through Rust.
  4. **The AI safety boundary is kept:**
     - AI chat output never loads remote images. They're shown as links, so a model can't exfiltrate data through an image URL while you read its answer.
     - The AI change preview (Phase 6) shows remote images as URLs, not images, and flags every *new* external image a proposal would add, for example: "This change adds an image from example.com, which will load whenever the page is shown." Approving a change therefore can't silently plant a tracking or exfiltration URL.
- **Alternatives:**
  - Keep ADR-025's placeholders. The most private option, but pages look incomplete.
  - Placeholders with a per-image "Load" button. A middle ground, but with more friction and more work.
- **Consequences:**
  - Opening a page that contains web images contacts those servers, which reveals the user's IP address and when the page was opened. Such images can work as tracking pixels.
  - Offline, those images don't show.
  - Risk R17 tracks this.
  - A setting to switch remote images off could be added later. It isn't added now, to keep settings minimal.

## ADR-032: The Phase 2 page index lives in memory; SQLite arrives with search

- **Status:** Accepted; extended by ADR-035 (the in-memory index stays as the session's page tree, now loaded from the SQLite cache). **Date:** 2026-11-02 (Phase 2)
- **Context:** ARCHITECTURE §5 places the index in SQLite. Phase 2 needs only the page tree and each page's facts (ID, title, icon, revision, status). Full-text search, backlinks and collections, which are what SQLite is for, start in Phase 3.
- **Decision:** In Phase 2 the Document Service keeps the index in memory. Opening a workspace scans it. A rescan re-reads only files whose `(size, mtime)` signature changed, and reuses everything else, so outside edits cost a stat walk. Phase 3 adds the SQLite database from ARCHITECTURE §5 as a persistent cache of the same facts, plus FTS5.
- **Alternatives:** SQLite from the start. It adds a schema, migrations-by-rebuild and a second writer before anything needs them.
- **Rationale:** It is the smallest correct thing for this phase. The files stay the only source of truth either way, and nothing is lost when the process exits.
- **Consequences:** Opening a very large workspace reads every Markdown file's frontmatter once per launch until Phase 3. Opening a folder with more than 10,000 entries asks first (§5.1 task 4 of the implementation plan).

## ADR-033: The editor's saves are checked against the body revision

- **Status:** Accepted, approved by the user on 2026-11-02. It refines CLAUDE.md architecture rule 7, which was updated to match. **Date:** 2026-11-02 (Phase 2)
- **Context:** With ADR-009 as written, the editor's base revision covers the whole file. Renaming a page, changing its icon or favoriting it rewrites the frontmatter, which changes the file revision. The next autosave of the body would then report a conflict that isn't one, and so would a tag change made by another app.
- **Decision:**
  1. `get_page` returns both `rev` (the whole file) and `body_rev` (the BLAKE3 hash of the body only, truncated to 16 hex characters).
  2. `save_page(id, base_body_rev, body)` re-reads the file just before writing and compares the **body** revision. If the body changed, nothing is written and the result is `Conflict { current_body_rev, current_body }`. If only the frontmatter changed, the new body is written under the current frontmatter, so neither side's change is lost.
  3. Metadata edits (`set_title`, `set_icon`, `set_favorite`) take no base revision. Each changes one key surgically in the file as it is on disk at that moment. If that content came from outside Forma, it is snapshotted to history first (ARCHITECTURE §4.6).
  4. There is no separate `workspace://conflict` event. A conflict surfaces as the result of a save, or when `workspace://changed` reports a new body revision for the page being edited.
- **Alternatives:** Keep whole-file revisions and refresh the editor's base after each metadata change. That races with autosave and still turns every outside tag edit into a conflict banner.
- **Rationale:** A conflict should mean that two people changed the same text. Body edits are still never silently overwritten. A title or icon change is a single explicit user action that is applied to the current file, and the snapshot policy keeps whatever it replaced.
- **Consequences:** CLAUDE.md rule 7 now reads: "Revisions are content hashes. Every body write carries the base body revision. Metadata edits are surgical, applied to the current file, and snapshot outside content first." AI proposals (Phase 6) still carry a whole-file `base_rev`, as ARCHITECTURE §10 specifies.

## ADR-034: Outcome of the Markdown gate: `@tiptap/markdown` plus a Forma rules layer

- **Status:** Accepted. **Date:** 2026-11-02 (Phase 2)
- **Context:** ADR-015 made `@tiptap/markdown` conditional on the round-trip corpus passing.
- **Decision:** Adopt `@tiptap/markdown` 3.32 (marked 17), with Forma's rules in `src/features/editor/markdownRules.ts`, `math.ts` and `rawBlock.ts`. Run as shipped, the library failed the corpus in these ways, each now fixed by a rule and covered by a fixture:
  1. Code fences weren't lengthened when the code contained a fence, which corrupted the block.
  2. Paragraph lines starting with `#`, `>`, `-` or `1.` weren't escaped, so they came back as blocks.
  3. `&`, `<` and `>` were written as HTML entities.
  4. Every `_` was escaped, even inside words.
  5. Autolinks such as `<https://…>` were rewritten as `[…](…)`.
  6. Hard breaks were written as two trailing spaces, which editors strip. Forma writes a backslash.
  7. Tables got a blank line before them and an extra one after them, and the alignment row was wider than its columns.
  8. `$5 and $10` was read as math. Forma follows the pandoc rule for inline math.
  Raw HTML, footnotes and wikilinks are kept byte for byte as atom nodes and never rendered.
- **Alternatives:** The `prosemirror-markdown` fallback. The problems found were local serializer rules, not structural, so the fallback would have cost more than the fixes.
- **Rationale:** It is the maintained path, and the rules layer is small and fully covered by the corpus (23 fixtures; byte-identical output for canonical input).
- **Consequences:** Upgrading `@tiptap/markdown` must re-run the corpus. Some valid Markdown is written back in Forma's canonical form, for example `*` list markers become `-` and tables are padded to aligned columns. Opening such a page shows a notice, and the original is snapshotted before the first save rewrites it (ARCHITECTURE §6.3).

## ADR-035: The SQLite cache: loaded at open, written with every change, one connection

- **Status:** Accepted. **Date:** 2026-11-03 (Phase 3)
- **Context:** Phase 3 needs full-text search, backlinks and collection values across the workspace, and a fast second open. ARCHITECTURE §5 describes a SQLite cache with a writer task and a read pool.
- **Decision:**
  1. The cache lives at `~/Library/Application Support/<bundle-id>/index/<hash of workspace ID and path>/index.db` (ADR-003). It holds rows for pages, tags, links and the parsed frontmatter, plus two FTS5 tables (DATA_MODEL §9).
  2. Opening a workspace loads the cached rows as the starting point of the scan, so files whose size and modification time are unchanged aren't read. The in-memory index (ADR-032) stays the session's page tree.
  3. Every rescan, and every file Forma writes, updates the cache in one transaction, with the content of each file that was read.
  4. One connection, owned by the Document Service and used under its lock. A read-only workspace (open in another window) keeps its index in memory. A broken cache file is deleted and rebuilt.
  5. "Rebuild search index" re-reads every file into a side database and swaps it in when complete. It keeps page IDs that exist only in memory, so open pages don't change identity.
- **Alternatives:** A dedicated index task with a read pool, so search keeps working during a rebuild and long indexing. Not needed at the measured cost (cold open of 5,000 pages 0.6 s, rebuild 0.6 s, searches 0.2–5 ms).
- **Rationale:** The smallest design that meets the acceptance criteria, with one writer and no cross-thread coordination.
- **Consequences:** During a rebuild or a cold first open, other workspace requests wait (under a second at 5,000 pages). A test checks that a rebuilt index equals the incrementally maintained one, row for row. If workspaces grow much larger, move indexing to its own task.

## ADR-036: CJK search uses a trigram table with LIKE; words use unicode61

- **Status:** Accepted. Resolves risk R6. **Date:** 2026-11-03 (Phase 3)
- **Context:** The `unicode61` tokenizer treats a run of Chinese, Japanese or Korean text as a single token. In the Phase 3 spike, searching `注意力` in a sentence that contains it found nothing; only the entire sentence matched. Many Chinese words are two characters long, below the trigram minimum.
- **Decision:**
  1. Keep `pages_fts` (unicode61) for words: every term must match, the last as a prefix, ranked by BM25 with the title weighted highest.
  2. Add `pages_trigram` (trigram, diacritics removed) holding title, body and property text.
  3. A query with any CJK character uses `LIKE '%term%'` on the trigram table for every term. Three or more characters use the trigram index; one or two characters scan, bounded by the result limit. Snippets are cut in Rust.
  4. A word query with no results whose terms have three or more characters falls back to the same substring search (`tention` finds `attention`).
- **Measured** (5,000 synthetic pages, 20 MB of Markdown, release build): word queries 3.5–5 ms; `蛋白质` 0.3 ms; `知识` (two characters) 0.3 ms; substring fallback 0.25 ms. The index is 35 MB on disk, about 1.7 times the Markdown, mostly the trigram table.
- **Alternatives:**
  - A dictionary segmenter (jieba-style) for Chinese. Better relevance, but a large dependency and still wrong for names and new words.
  - Trigram only. Simpler, but it loses word prefixes and BM25 ranking for English.
  - Unicode61 only. Fails the CJK user entirely.
- **Rationale:** Correct results for CJK at negligible cost, with no new dependency.
- **Consequences:** CJK results are substring matches in no particular relevance order (title matches aren't ranked first). The index is larger than with one table. Revisit ranking for CJK if users ask.

## ADR-037: Collections in the sidebar and in pages

- **Status:** Accepted, following the plan's default proposal. **Date:** 2026-11-03 (Phase 3)
- **Context:** IMPLEMENTATION_PLAN §6.1 asks to decide how a collection's entries show in the sidebar. A collection can hold thousands of entries.
- **Decision:**
  - In the sidebar, a collection shows with a table icon, and its entries are its children: collapsed by default and loaded when expanded, like any page. At most 50 are listed, followed by "N more — open the collection".
  - A collection page shows its text, then the collection (table or list view). The table renders only the rows on screen.
  - An entry page shows its properties under its title, with a link to its collection, and they're editable there too.
  - "Turn into a collection" is in the page menu. "Turn into a regular page" is in the collection's menu and asks first.
- **Alternatives:** Hiding entries from the sidebar entirely. Simpler, but entries then can't be dragged or reached from the tree.
- **Consequences:** Entries past the 50th are reached through the collection, search or quick open.

## ADR-038: Provider APIs as verified in Phase 4

- **Status:** Accepted. The OpenAI choice was the user's (2026-11-03); it changes ADR-019's OpenAI preset. Resolves risk R8 for now. **Date:** 2026-11-03 (Phase 4)
- **Context:** ADR-019 asked Phase 4 to check the providers' current APIs first. The official documentation was read on 2026-11-03:
  - **OpenAI:** Chat Completions isn't deprecated, but OpenAI recommends the Responses API for new projects, and its reasoning guide says "Chat Completions does not support function calling with GPT-6 Astra or GPT-6.1 Sol". Act mode (Phase 6) needs function calling.
  - **Anthropic:** the latest API version is `2026-10-21`. It removes `temperature`, `top_p` and `top_k` for every model, so sending any of them is a 400 error. `Authorization: Bearer <API key>` is the documented header; `x-api-key` still works as a "legacy fallback". `max_tokens` is required. The model list (`GET /v1/models`, paged with `after_id`) and `GET /v1/models/{id}` give `max_input_tokens` and `max_tokens`.
  - **Ollama:** `/api/chat` streams JSON lines. The default `num_ctx` is 2048 tokens, so long input is cut silently unless a window is set. `/api/show` lists `capabilities` and `<architecture>.context_length`. Tool calls arrive whole, with arguments as an object.
  - **LM Studio:** chat through the OpenAI-compatible `/v1/chat/completions`. The native `GET /api/v1/models` adds `max_context_length`, the loaded context length and `trained_for_tool_use`.
- **Decision:**
  - The OpenAI preset uses the **Responses API** (`POST /v1/responses`) with `store: false`, so OpenAI doesn't keep the conversation for later retrieval. OpenAI-compatible servers and LM Studio keep Chat Completions.
  - Anthropic requests send `anthropic-version: 2026-10-21` and `Authorization: Bearer`, never a temperature (the setting is hidden for Anthropic), and `max_tokens` of 4096 unless the user sets one.
  - Ollama requests always send `num_ctx`: the user's override, else the model's context length capped at 8,192 tokens (a bigger window takes more memory).
  - A temperature is sent only when the user sets one. Several models reject it, and the error then names the parameter.
- **Alternatives:** Chat Completions for OpenAI too (ADR-019 as written). Less code, but the newest OpenAI models couldn't use Act mode. Pinning Anthropic's `2023-06-01` version, which still takes a temperature: Anthropic recommends the latest version, and the parameter is gone for every model there.
- **Consequences:** Three wire protocols instead of two. The stream fixtures follow the documented formats; they aren't recordings, because no API keys were available in Phase 4. A test that's ignored by default (`tests/live_endpoints.rs`) checks TLS, the auth headers and real 401 responses from both cloud APIs.

## ADR-039: The AI service: one provider type, rules in Rust, answers over a Channel

- **Status:** Accepted. **Date:** 2026-11-03 (Phase 4)
- **Context:** ARCHITECTURE §9.1 sketched an async `ChatProvider` trait object. In Rust 2024 an `async fn` in a trait can't be used as `dyn` without boxing every future, and the four protocols differ only in URLs, request bodies and stream decoding.
- **Decision:**
  - One `Provider` type sends requests for any configured provider and picks the protocol from its kind. Each protocol is a module with a request builder and a **pure decoder** (stream frames in, `ChatEvent`s out), so recorded streams test it without a network.
  - `AiService` (held by the shell for the app's lifetime) owns every rule about sending: AI must be on, a provider and model chosen, a key present where required, and agreement given for a provider that isn't on this Mac. It also estimates the conversation's size and refuses one that can't fit the context window, rather than letting the provider cut it.
  - Keys are read from the Keychain once per session and kept in Rust memory. Keychain calls run on blocking threads, because macOS may show a prompt. If the Keychain can't be opened, keys last for the session only, and the log says so.
  - Answers stream to the webview over a Tauri Channel. The request ID comes from the webview, so Stop works even if it arrives before the request starts. Cancelling drops the HTTP connection.
  - Errors carry a kind (auth, unreachable, TLS, timeout, rate limit with `retry_after`, model missing, context overflow, provider, interrupted, cancelled, not configured, disabled) and a message for the user. Any copy of the key in a provider's message is replaced before it leaves Rust. Logs record the kind and status, never the message, which can quote what was sent.
  - At the end of a stream, an unfinished line or event is dropped (as the event-stream rules say), and an answer without its final event is reported as interrupted.
- **Alternatives:** A trait object per protocol with boxed futures. More code for the same behavior, and the rules about sending would be spread over the adapters.
- **Consequences:** Adding a protocol means a module and one arm in each `match`. A provider that needs more than a URL, a body and a decoder would bring the trait back.

## ADR-040: Chat answers are rendered from Markdown tokens as React elements

- **Status:** Accepted. **Date:** 2026-11-03 (Phase 4)
- **Context:** AI output is untrusted (ARCHITECTURE §10.4). It must render as Markdown with no HTML, no remote images and no `javascript:` links.
- **Decision:** `marked` 17.0.6, already installed through `@tiptap/markdown` and now a direct dependency at that version, only tokenizes the answer. Forma builds React elements from the tokens, so nothing passes through `innerHTML`. Raw HTML appears as text. An image shows as "[Image: alt · address]" and isn't loaded. Only `https:`, `http:` and `mailto:` links can be opened: they're buttons that name their destination and open it in the browser on click. Other links show their address as text.
- **Alternatives:** `marked` to HTML plus a sanitizer (DOMPurify). A sanitizer bug or setting would become an XSS hole, and XSS here could defeat the approval boundary (ARCHITECTURE §2.1). A read-only Tiptap editor: heavy for a chat answer.
- **Consequences:** The renderer handles the token types it knows (headings, paragraphs, lists and task lists, code, quotes, tables, rules, and the inline forms). Anything newer shows as its source text. HTML entities written by the model (for example `&amp;`) show as written.

## ADR-041: Retrieval as built: passage chunks, stemmed BM25, CJK through page trigrams, rank fusion

- **Status:** Accepted. Implements ADR-022. **Date:** 2026-11-03 (Phase 5)
- **Context:** The assistant needs the passages of the notes that bear on a question, for any model and offline. Questions are sentences, not keywords, and many of the user's notes are in Chinese, which unicode61 can't split into words (ADR-036).
- **Decision:**
  - **Passages.** A page body is cut at headings and between blocks into passages that close once they reach 250 tokens and never exceed 400 (long blocks are cut at lines, then sentences, then characters). Headings with nothing under them yet stay with what follows. Each passage keeps its heading path, its byte range in the body, its Markdown and its plain text. Index schema 2 adds `passages` and `passages_fts` (title, headings and text, `porter unicode61`, so "folded" finds "fold"); an older index is rebuilt automatically.
  - **Words.** The question's words, without English stop words, are OR-ed in one FTS5 query, ranked by BM25 with the title and headings weighted above the text.
  - **CJK.** The question's CJK runs become three-character pieces. The existing page trigram table finds the pages sharing the most pieces (BM25); their passages are then ranked in Rust by how many pieces each contains. Two-character words ("知识") fall back to a bounded scan.
  - **Fusion.** The two rankings are merged by reciprocal rank (k = 60). At most three passages come from one page. `Retriever::retrieve(query, k, filter)` is the interface; `ReadView` implements it.
- **Alternatives:**
  - A second, passage-level trigram table. Measured on 5,000 pages: cold open 2.2 s and a 71 MB index, against 0.83 s and 51 MB without it, with the labeled questions answered as well.
  - Requiring every word to match, as ⌘K does. Questions contain words the notes don't ("how", "should").
  - Embeddings. Out of scope for v0.1 (ADR-022).
- **Consequences:** Recall rests on shared words or characters: a paraphrase with none in common finds nothing, and then the assistant says the notes don't cover it. On 5,000 pages, a cold open takes about 0.2 s longer than in Phase 4 (0.83 s against 0.63 s, same machine), a rebuild 0.68 s, and the index is 46% larger (51 MB). A labeled set in `tests/fixtures/retrieval/` (17 questions in English and Chinese, one with no answer) guards the ranking.

## ADR-042: Asking about notes: numbered sources, a packed context, read-only tools, and a new cloud agreement

- **Status:** Accepted. **Date:** 2026-11-03 (Phase 5)
- **Context:** Phase 5 lets the assistant read the notes. What it reads must be visible, citable and bounded by the model's window, and it must not be able to write.
- **Decision:**
  - **Read-only by type.** The assistant reaches the workspace only through `forma_core::workspace::read::ReadView`, which has no method that writes (`Workspace` has a few `&self` methods that write recovery buffers and snapshots; they aren't on it). `forma_ai::knowledge::Library` hands out nothing else. Each Ask tool's `run` takes a `ReadView` and its arguments.
  - **Sources.** Every piece of the notes sent to a model is a `<source key="S#" kind page page_id section>` block, keyed for the conversation (keys continue across messages). A source gets its key only if its text is actually sent. Source text can't close its block, and the system prompt says sources are data, not instructions. The panel receives the sources from Rust and checks each citation in an answer against the sources of that answer; any other key is marked unverified and links nowhere.
  - **Packing.** Within the window less the answer's reserve: the system prompt and the new message, then the selection (up to 30% of what's left), the attached page (up to 60%; a long page is cut to the passages nearest the cursor, gaps marked "[…]"), retrieved passages (up to 60% of what's left), then earlier turns, newest first. Rust reports what went in (page, selection, passage count, turns left out). A window that neither the user nor the provider gives counts as 8,192 tokens.
  - **Tools.** Models that can call tools (as the provider reports, or as the user sets in Settings → AI) also get `search_pages`, `read_page`, `list_pages` and `get_page_metadata`: typed JSON Schemas with no extra properties, page IDs only (checked as UUIDs, looked up in the index), results cut to 3,000 tokens and to the room left, at most 6 rounds and 10 minutes, then one round that must answer. That last round keeps the tools defined with `tool_choice: none`, because some APIs reject a conversation that uses undefined tools. Anthropic's thinking blocks are passed back unchanged with tool results, as its API requires.
  - **Search notes switch.** On by default. Turned off, nothing from the notes is sent unless the user attaches a page or a selection, and no tools are offered, since tools read notes too.
  - **A new cloud agreement.** Phase 4's agreement said Forma sends "never your notes". Agreements now live in `ai.cloud_consent`, and the text says that the parts of the notes a question needs go to the provider. Agreements recorded in Phase 4's `cloud_acknowledged` aren't carried over, so a cloud provider asks once more.
- **Alternatives:**
  - Sending sources in the system prompt. Providers recommend documents in the user's turn; it also keeps each message's sources next to it.
  - Re-numbering sources from S1 in every message. Answers in the history would then cite keys that now mean something else.
  - Carrying the Phase 4 agreement over. It was given on the promise that notes are never sent.
- **Consequences:** No real provider has seen these requests yet (no keys or local models were available), so the tool message formats follow the documentation. Citations depend on the model following the instructions; one that doesn't cite shows its answer without sources, and the panel says which sources it was given.

