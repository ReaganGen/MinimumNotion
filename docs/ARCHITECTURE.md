# Forma Architecture

Status: Phase 0 baseline (2026-11-02). This document describes the intended system design for v0.1.0. Where a later phase changes a design, update this file and record the change in [DECISIONS.md](DECISIONS.md).

Related documents:

- [DATA_MODEL.md](DATA_MODEL.md): exact on-disk formats, entity fields, the Markdown dialect, and the index schema
- [DECISIONS.md](DECISIONS.md): architectural decision records (ADRs)
- [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md): per-phase tasks and acceptance criteria

---

## 1. Goals and non-goals

The architecture serves these properties, in priority order:

1. **Correctness and data integrity.** No user-authored content is lost or silently overwritten. Every on-disk write is atomic. Every destructive operation can be recovered.
2. **Local-first and portable.** The workspace is a plain folder of Markdown files with YAML frontmatter. A user who stops using Forma keeps everything, and the folder structure stays readable.
3. **Simplicity.** Each concept has one authoritative home. Most mutations are a single atomic filesystem operation.
4. **Optional, safe AI.** AI is a separate subsystem with no direct write path to user data. It can only propose changes, and the user applies them.

Non-goals for v0.1.0 are listed in [ROADMAP.md](ROADMAP.md#explicitly-out-of-scope-for-v010). The architecture keeps extension points for them (§13), but none of them is built.

---

## 2. System overview

```
┌──────────────────────────── Forma.app (single process, single window) ───────────────────────────┐
│                                                                                                  │
│  WebView (WKWebView) — React + TypeScript                     Rust (Tauri 2 shell)               │
│  ┌──────────────────────────────────────────┐   invoke()      ┌───────────────────────────────┐  │
│  │ Sidebar │ Editor (Tiptap/PM) │ AI panel  │ ──────────────▶ │ commands/  (thin IPC layer)   │  │
│  │                                          │ ◀────────────── │ events / channels             │  │
│  │ Markdown body ⇄ ProseMirror conversion   │   events,       └──────────────┬────────────────┘  │
│  │ UI state (zustand)                       │   channels                     │                   │
│  │ NO filesystem; network: https images only│                 ┌──────────────▼────────────────┐  │
│  └──────────────────────────────────────────┘                 │ forma-core (no network)       │  │
│        ▲ forma-asset:// (read-only, workspace-scoped)          │  Workspace  · DocumentService │  │
│        └──────────────────────────────────────────────────────│  Index (SQLite/FTS5) · Watcher│  │
│                                                                │  Collections · History · Trash│  │
│                                                                └──────────────▲────────────────┘  │
│                                                                ┌──────────────┴────────────────┐  │
│                                                                │ forma-ai (network)            │  │
│                                                                │  Providers · Agent · Tools    │  │
│                                                                │  ChangeSets (proposals only)  │  │
│                                                                └───────┬───────────────┬───────┘  │
└────────────────────────────────────────────────────────────────────────┼───────────────┼──────────┘
                                                                         │               │
             ~/FormaWorkspace/ (user's Markdown files, .forma/)  ◀───────┘   HTTPS / localhost
             ~/Library/Application Support/<bundle-id>/ (settings, index.db)    to AI providers
             macOS Keychain (API keys)
```

### 2.1 Process model and trust boundaries

| Component | Trust | May access |
|---|---|---|
| Rust backend (`forma-core`, `forma-ai`, shell) | Trusted | Workspace folder, app data dir, Keychain, network (only `forma-ai`) |
| WebView application code (our bundle) | Trusted code that renders untrusted content | Only the IPC commands allow-listed in Tauri capabilities, plus the `forma-asset://` protocol |
| Document content (Markdown, images) | **Untrusted** | Nothing. Never executed or rendered as raw HTML |
| AI model output and tool arguments | **Untrusted** | Only read tools and proposal tools (§10) |
| Files modified externally | **Untrusted input** | Parsed defensively and validated |

Consequences:

- The webview has no network access of its own. The Content Security Policy sets `connect-src` to IPC only and allows no remote `script-src`, `style-src`, `font-src` or `frame-src`. The one exception is `img-src https:`: `https:` images embedded in notes are loaded (ADR-031, from Phase 2). All AI traffic goes through Rust, so API keys never enter JavaScript memory.
- The webview has no generic filesystem plugin. Every file operation is a typed command whose arguments are **page IDs, not paths**. Rust computes paths from the index and checks that each one stays inside the workspace. This removes path traversal from the IPC surface.
- Paths from outside the workspace come only from native user actions handled in Rust: the folder picker (Rust opens the dialog itself) and files dropped onto the window. A dropped file reaches the webview as an opaque, single-use **drop token** issued by Rust, never as a path. Compromised webview code therefore can't ask Rust to copy arbitrary files, such as `~/.ssh/*`, into the workspace.
- Raw HTML in documents and in AI output is shown as escaped text, never rendered. This matters for integrity as well as privacy: script running in the webview could call `apply_changeset`, so an XSS bug would defeat the AI approval boundary. A strict CSP (no `unsafe-inline` or `unsafe-eval` for scripts) adds a second layer of defense.

### 2.2 Repository layout

```
/                         (repository root; product name "Forma")
├── CLAUDE.md, README.md, .gitignore
├── package.json, vite.config.ts, tsconfig.json, biome.json, index.html      (Phase 1)
├── src/                  React application
│   ├── app/              App shell, providers, routing between views
│   ├── components/       ui/ (shadcn primitives), layout/, sidebar/, editor/, collections/, ai/, search/
│   ├── features/         Feature logic + stores: pages/, collections/, search/, settings/, ai/
│   ├── lib/              ipc/ (typed client + generated bindings), markdown/, utils
│   └── styles/           tokens.css (design tokens), globals.css
├── Cargo.toml            Cargo workspace
├── crates/
│   ├── forma-core/       Workspace, DocumentService, frontmatter, index, watcher, collections. No Tauri, no network.
│   └── forma-ai/         Provider adapters, agent runtime, tool registry, ChangeSets. Depends on forma-core.
├── src-tauri/            Thin Tauri shell: commands, events, window, menus, capabilities, bundling
├── tests/fixtures/       Shared fixtures, including the Markdown round-trip corpus used by TS and Rust tests
├── scripts/              generate-workspace.mjs (performance fixtures), verify-bundle.sh (release checks)
├── docs/                 USER_GUIDE.md for users; the rest for development
├── CONTRIBUTING.md
└── .github/workflows/    ci.yml (checks, audits, bundle build), release.yml (draft release on a version tag)
```

There are two reasons to split `forma-core` from the Tauri shell. First, `cargo test -p forma-core` runs without compiling WebKit bindings, which keeps the test loop fast. Second, core logic stays platform-independent, so a future CLI, sync daemon or other OS port can reuse it. `forma-ai` is separate so that `forma-core` has no network dependency at all, which keeps the security boundary visible in `Cargo.toml`.

Directories are created only when they get real content. No empty placeholder modules.

---

## 3. Workspace on disk

Full specification: [DATA_MODEL.md §2–§4](DATA_MODEL.md#2-workspace-directory-layout).

```
MyWorkspace/                         ← the folder the user picked; the page tree root
├── Inbox.md                         ← leaf page (no children, no attachments)
├── Research/                        ← bundle page: a page with children or attachments
│   ├── index.md                     ←   the "Research" page itself
│   ├── _assets/                     ←   attachments of "Research" (referenced as _assets/x.png)
│   │   └── diagram.png
│   ├── Paper A.md                   ←   child page (leaf)
│   └── Papers/                      ←   child page that is a Collection (bundle)
│       ├── index.md                 ←     collection schema lives in this page's frontmatter
│       ├── Attention Is All You Need.md   ← collection entry (child of the collection page)
│       └── ...
└── .forma/                          ← Forma metadata (portable, human-readable, no cache)
    ├── workspace.json               ← workspace identity + format version
    ├── tree.json                    ← sibling order + favorites (advisory)
    ├── views.json                   ← collection view settings (advisory)
    ├── history/<page-id>/…          ← revision snapshots (full Markdown files)
    ├── trash/<entry-id>/…           ← trashed pages with their whole subtree
    ├── operations/<op-id>.json      ← applied AI ChangeSets (for undo)
    ├── journal/                     ← write-ahead intents for multi-file batches
    ├── recovery/                    ← unsaved editor buffers kept during conflicts
    ├── tmp/                         ← staging area for atomic writes (same volume)
    └── lock                         ← advisory lock: one Forma process per workspace
```

Outside the workspace, in `~/Library/Application Support/<bundle-id>/`:

- `settings.json`: app preferences (theme, last workspace, AI provider configuration without secrets)
- `index/<workspace-key>/index.db`: the SQLite index, which is a **cache** (§5)
- `logs/`

API keys live in the macOS Keychain only.

### 3.1 Why the directory tree mirrors the page tree

The folder hierarchy *is* the page hierarchy, so a page's parent is derived from its location and is never stored twice. Most operations become one atomic `rename(2)`:

| Operation | Filesystem effect | Atomic? |
|---|---|---|
| Move page (with subtree) | `rename` of one file or one bundle directory | Yes |
| Rename page | rewrite frontmatter `title` (atomic write), then `rename` of the file or directory | Each step is atomic. A crash between them leaves a stale filename, which is harmless because the filename is not authoritative |
| Trash page (with subtree) | write `entry.json` into a new trash entry, then `rename` the file or bundle into it | Yes (an orphaned empty entry is cleaned up on start) |
| Restore from trash | `rename` back | Yes |
| First child or attachment on a leaf | `mkdir Name/`, then `rename Name.md → Name/index.md` | Each step is atomic. A crash between them leaves an empty `Name/` beside `Name.md`, which startup recovery removes |
| Duplicate (subtree) | copy into `.forma/tmp/`, assign new IDs, then one `rename` into place | Yes (publish by rename) |

Attachments live in their page's own `_assets/` folder and are referenced relative to `index.md`. Their references therefore never change when the page is renamed or moved. Page-to-page links use stable IDs (§6.5), so renames and moves never rewrite other files.

Only true multi-file batches need the write-ahead journal: renaming a collection property or select option (rewrites every entry), and applying an AI ChangeSet. One rare single-page case also uses it: converting a leaf whose body contains hand-written relative references, which must be adjusted (DATA_MODEL §3.1).

---

## 4. Document Service

`forma-core::documents::DocumentService` is the **only** component that writes workspace files. The UI, the Tauri command layer and the AI agent all call it. None of them touch the filesystem.

### 4.1 Responsibilities

- Workspace open, initialize and close, plus the workspace lock and startup recovery
- Page reads, which return metadata, the Markdown body and the current revision
- All mutations: create, save, set title, move, reorder, duplicate, trash, restore, favorite, icon, tags, attachments, collection schema and properties, revision restore, and batched ChangeSet application
- Atomic persistence, revision checks, history snapshots and the trash
- Recording its own writes so the watcher can tell them apart from external changes
- Telling the index about every change, and emitting change events to the UI

### 4.2 Concurrency model

- **Single writer.** One mutation task per workspace processes requests in order from a queue, so mutations never interleave. Reads (page content, index queries) run concurrently with the writer and never wait for it. Because files are replaced by atomic rename, a reader always sees either the complete old file or the complete new one.
- **Frontend write serialization.** The frontend keeps a per-page write queue so a body save and a metadata patch for the same page never race. Each request carries the latest known revision.
- **One process per workspace.** `tauri-plugin-single-instance` plus an advisory `flock` on `.forma/lock`. If the lock is held, the workspace opens read-only and explains why.

### 4.3 Mutation pipeline

Every mutation follows the same steps:

```
validate request (IDs exist, names valid, no cycles, revision matches)
  → [multi-file batch only] write journal intent to .forma/journal/
  → snapshot previous content to history (per policy, §4.6)
  → atomic write(s) / rename(s)
  → register self-writes (path, revision) for watcher suppression
  → update index in one SQLite transaction (failure → mark index stale; never fails the mutation)
  → [batch only] delete journal intent
  → emit `workspace://changed { pages: [{id, rev}], removed, tree_parents }`
```

### 4.4 Atomic write protocol

1. Write the new bytes to `.forma/tmp/<random>`. This is on the same volume, so the later rename is atomic, and it keeps temporary files out of the user's folders and out of sync tools.
2. Call `File::sync_all()`. On macOS, Rust's standard library implements this with `fcntl(F_FULLFSYNC)`.
3. `rename(tmp, target)`. This is atomic on APFS and HFS+.
4. `fsync` the parent directory.

Startup removes anything left in `.forma/tmp/`. A crash at any point leaves either the old file or the new one, never a torn file.

If the workspace and `.forma/tmp` turn out to be on different volumes (cross-device rename fails with `EXDEV`), the service falls back to a temporary file in the target directory, named `.<name>.forma-tmp-<random>`, which the watcher and the scanner ignore.

### 4.5 Revisions and optimistic concurrency

- A page's **revision** (`rev`) is the BLAKE3 hash of the file's full bytes (frontmatter plus body), truncated to 16 hex characters. It is derived, never stored, and it changes automatically when anything edits the file, including external tools.
- Every write to an existing page carries `base_rev`. Just before replacing the file, the service re-reads it and compares hashes. If they differ, the write is rejected with `Conflict { current_rev, current }` and nothing is written.
- **The editor's saves compare the body only** (ADR-033). `save_page` carries `base_body_rev`, the hash of the body the editor started from. A frontmatter change made meanwhile (a rename, a tag edit in another app) isn't a conflict: the body is written under the current frontmatter. Title, icon and favorite edits take no base; each is a surgical one-key change to the current file, after snapshotting content that came from outside.
- Accepted residual risk: an external process could write between the hash check and the `rename` (a window of microseconds to milliseconds). POSIX offers no cross-process compare-and-swap for files. The pre-overwrite snapshot policy (§4.6) narrows the impact, and this risk is documented rather than claimed away.

### 4.6 History (Document Revisions)

History snapshots are full copies of the file bytes, stored at `.forma/history/<page-id>/<UTC timestamp>_<rev>_<reason>.md`. They stay readable without Forma.

The service snapshots the current on-disk content before overwriting it whenever any of the following is true:

- that content was not produced by Forma's own last write. This covers external edits and first-time edits of files created outside Forma, so the original bytes are always recoverable.
- the page's most recent snapshot is older than the coalescing window (default 10 minutes of editing)
- the write is an AI ChangeSet apply, a revision restore, a conflict resolution, an undo, or a schema batch rewrite (reason-tagged "protected" snapshots)

Retention is conservative. Phase 2 sets and documents the thinning rules. Snapshots younger than 30 days are never pruned, and snapshots referenced by an undoable AI operation are kept while that operation can still be undone.

### 4.7 Trash

Trashing moves a page's file or bundle, with its whole subtree, into `.forma/trash/<entry-id>/` together with `entry.json`, which records the original parent ID, the original relative path, the time and the page IDs in the subtree. Descendants are never destroyed separately.

Restoring returns the subtree to its original parent if that parent still exists outside the trash, and to the root otherwise, adjusting the name if it collides. Permanent deletion ("Delete forever" or "Empty trash") is a separate, explicitly confirmed user action. v0.1 never purges the trash automatically.

### 4.8 Page identity and ID persistence

- Every page has a UUID stored in frontmatter as `id`. Identity never depends on title or path.
- **Lazy persistence.** A file discovered without an `id` (for example, Markdown the user dropped into the folder) gets an *index-only* ID. Forma writes the `id` into that file only when it writes the file for another reason, or just before anything durable starts referencing the page (a link, a favorite, an order entry, a history snapshot, an operation record or a journal step; the full list is in DATA_MODEL §4.4). Opening an existing folder of Markdown therefore never mass-rewrites the user's files.
- **Duplicate IDs** can appear when a user copies a file in Finder or a sync tool creates conflict copies. When two files carry the same `id`, the file at the path the index already associates with that ID keeps it. Without an index entry, the earliest-created file keeps it, with path order as the tie-breaker. The other file receives a new UUID, written into its frontmatter, and the user is told. Both files are kept.

### 4.9 External changes

- A `notify` watcher on the workspace root (FSEvents, recursive, debounced about 300 ms) treats events as *hints*. For each affected path, the service stats the file, hashes it and reconciles. Events that match a registered self-write are ignored.
- Moves and renames are recognized by `id`: the same ID disappearing at one path and appearing at another. Open editors follow the page transparently.
- Interaction with an open editor:

| Editor state | External event | Behavior |
|---|---|---|
| Clean | Modified | Reload silently, preserving the selection where possible |
| Dirty | Modified | **Conflict.** Autosave pauses and the editor buffer is written to `.forma/recovery/<id>.md` at once. A banner offers *Keep mine* (disk version snapshotted first), *Use theirs* (buffer snapshotted first) or *Keep both* (the buffer becomes a sibling page "Title (conflict YYYY-MM-DD)") |
| Any | Moved/renamed (same ID) | Follow the page, with no interruption |
| Clean | Deleted | The page closes |
| Dirty | Deleted | Autosave pauses and the buffer is kept in recovery. A banner offers *Restore with my text* (a new page with the same title and parent, holding the buffer) or *Discard my text* |

- No external event ever causes Forma to overwrite a file without a fresh `base_rev` check.

### 4.10 Startup recovery

On open, the service:

1. Acquires the lock.
2. Clears `.forma/tmp/`.
3. Replays or rolls back any `.forma/journal/` intents. Each batch type defines its rollback, which is restoring the protected snapshots taken before the batch.
4. Removes empty bundle directories left by an interrupted leaf-to-bundle conversion. It removes a directory only when it is truly empty and sits beside a `Name.md`.
5. Cleans up orphaned trash entries.
6. Offers any `.forma/recovery/` buffers back to the user.
7. Reconciles the index (§5.3).

### 4.11 Export, backup and restore

Settings → Data (ADR-044). The shell opens the folder or file dialog itself and passes the chosen path to `forma-core::archive`, so no path crosses IPC. Export and backup run while the session holds the workspace for reading, so no save lands halfway through. Each writes under a staging name (`.forma-export-<uuid>`, `<name>.zip.partial`, `.forma-restore-<uuid>`) and renames when complete, so a failure leaves nothing under the final name.

- **Export:** the workspace's files without `.forma/`, into a new folder.
- **Backup:** a zip of the workspace with `.forma/`, without `.forma/tmp/` and `.forma/lock` (DATA_MODEL §7.8).
- **Restore:** validates every entry before writing (relative names only, no links, size and count limits), unpacks into a new folder and opens it. It never writes into an existing folder or the open workspace.

Diagnostics (`data_commands.rs`, `forma-core::diagnostics`): the log viewer shows the last 400 lines of `~/Library/Logs/<bundle-id>/forma.log`; "Copy diagnostics" builds a report of versions and a settings summary plus 80 log lines with quoted strings and paths replaced. Neither contains note text, titles, keys or server addresses.

---

## 5. Index (SQLite)

> **Phase 3 status (ADR-035):** the SQLite cache below is in place. The Document Service still keeps the session's page tree in memory (ADR-032); the cache is loaded as the starting point of each open, so only changed files are read, and every rescan or write updates it in one transaction.

### 5.1 Role

SQLite is a **derived cache**. It accelerates the page tree, full-text search, backlinks, collection sorting and filtering, and AI retrieval. Deleting it loses nothing: the service rebuilds it from the workspace files. No user-authored data is written only to SQLite.

- **Location:** `~/Library/Application Support/<bundle-id>/index/<workspace-key>/index.db`, where `workspace-key` is a hash of the workspace's `id` and its canonical path. Keeping it outside the workspace means a synced workspace (Dropbox, iCloud) never carries a SQLite database, which sync tools corrupt, and it keeps the user's folder free of cache files.
- **Engine:** `rusqlite` with the `bundled` feature (SQLite 3.54), so SQLite and its FTS5 support are pinned and do not depend on the system SQLite. WAL mode. In v0.1 one connection is owned by the Document Service and used under its lock; searches take milliseconds (PROGRESS.md), so a read pool isn't needed yet (ADR-035).
- **Read-only workspaces** (open in another window) keep their index in memory, so two windows never write the same cache. If the cache file can't be used, it's deleted and created again, and if that fails too, the index lives in memory for the session.
- **Schema versioning:** `meta.schema_version`. On a mismatch the index is dropped and rebuilt. The cache needs no migration code.

### 5.2 Contents

Pages (path, parent, title, icon, kind, revision, size and mtime for change detection, whether the ID is persisted, status, whether it's a collection, and the parsed frontmatter as JSON), tags, links (page links by target ID, wikilinks by their text), a word FTS5 table over title, body text, tags and property values, a trigram FTS5 table for substring and CJK search (ADR-036), and (schema 2, Phase 5) the passages the AI assistant retrieves from, with their own word FTS5 table (ADR-041). Collection values are coerced from the cached frontmatter when a collection is read, so there is no separate values table. Exact schema: [DATA_MODEL.md §9](DATA_MODEL.md#9-sqlite-index-schema).

Body text for FTS and link extraction comes from the Rust side, using `pulldown-cmark` (§6.3).

### 5.3 Reconciliation and rebuild

- **Startup reconcile.** Walk the workspace, skipping `.forma/`, dotfiles and symlinks, which are never followed. Compare each file's `(size, mtime)` with its index row, re-read only the files that changed, and detect moves by ID. With thousands of files this costs a stat walk, not a full parse.
- **Incremental.** The Document Service updates the index after its own writes. The watcher triggers reconciliation for external changes.
- **Rebuild.** Settings → Data → "Rebuild search index" re-reads every file into a fresh database beside the current one, and replaces the current one only when the new one is complete. In v0.1 the rebuild holds the Document Service's lock while it runs (0.7 s for 5,000 pages since Phase 5), so other requests wait instead of reading the old database (ADR-035).
- Indexing runs inside the Document Service's commands, which run off the UI thread. There is no `index://status` event in v0.1: the cold first open of 5,000 pages takes about 0.8 s (0.6 s before passages were indexed).

### 5.4 Search behavior

FTS5 with BM25 ranking, `snippet()` for previews, prefix queries for type-ahead, and title matches boosted (title ×10, tags ×4, properties ×2, body ×1).

- **Words** (any query without CJK characters): every term must match, the last as a prefix. If nothing matches and a term has three or more characters, the substring search below runs instead, so `tention` finds `attention`.
- **CJK** (any query with Chinese, Japanese or Korean characters): the `unicode61` tokenizer treats a whole run of CJK text as one token, so it can't find words inside sentences. These queries use the trigram table with `LIKE`: terms of three or more characters use the trigram index, and one- or two-character terms (common in Chinese) scan the table, bounded by the result limit. Snippets for these are cut in Rust. Measured at 5,000 pages: under 0.3 ms (ADR-036).
- Snippets mark matches with U+0001 and U+0002. The UI splits on them and renders text runs, never HTML.

---

## 6. Editor and Markdown pipeline

### 6.1 Division of responsibility

| Concern | Owner |
|---|---|
| File assembly: frontmatter plus body | Rust (`forma-core`) |
| Frontmatter parsing, validation and surgical editing | Rust |
| Body Markdown → ProseMirror document → body Markdown | TypeScript (Tiptap 3 + `@tiptap/markdown`) |
| Plain-text extraction, link and attachment extraction for the index | Rust (`pulldown-cmark`) |

The frontend never sees or produces frontmatter. It receives `{ meta, body, rev }` and sends back `{ body, base_rev }` or typed metadata patches. Because only Rust touches frontmatter, unknown keys, key order and comments survive (DATA_MODEL §4.6).

### 6.2 The editor

- **Tiptap 3** on ProseMirror. The title is a separate field above the editor and is stored as frontmatter `title`. It is not an H1 inside the body.
- Node and mark set: paragraph, heading (levels 1–6 accepted; only H1–H3 appear in the menus), bold, italic, strike, inline code, bullet, ordered and task lists, blockquote, code block (with a language attribute), link, image, GFM table, horizontal rule, hard break, inline and block math (KaTeX), page link, and the **raw block** described below.
- Underline is **disabled** because Markdown cannot represent it, so ⌘U must not create content that can't be saved. Generally, no editor feature ships without a serialization rule.
- **Raw block.** Markdown the editor can't represent (HTML blocks, footnote definitions, unknown syntax) becomes a read-only raw block that shows the source text and writes it back byte for byte. Unsupported content is never silently dropped.
- Interaction: Markdown input rules (`#`, `-`, `1.`, `[]`, `>`, ```` ``` ````, `---`, `$$`), a small slash menu, a contextual bubble menu for formatting and links, and `[[` for page-link autocomplete.

### 6.3 Round-trip guarantees

The Markdown dialect and its canonical serialization are specified in [DATA_MODEL.md §5](DATA_MODEL.md#5-markdown-dialect). The guarantees are:

1. **Semantic round-trip.** For every document in the fixture corpus, `parse → serialize` preserves semantics. For documents already in canonical form, it also preserves the bytes. Tests run on this corpus in Vitest (the TS serializer) and in Rust (text and link extraction must agree on the same fixtures).
2. **No write without an edit.** Opening a page never writes it. Forma writes only after a ProseMirror transaction with `docChanged` or an explicit metadata change. Externally authored files are never reformatted just by being viewed.
3. **First-write safety.** The first time Forma overwrites content it didn't produce, it snapshots the original first (§4.6). If the open-time check (`serialize(parse(body))` compared with `body`) finds differences beyond whitespace, the page shows a quiet notice: "Forma will reformat parts of this page when you edit it. View changes."
4. The editor library choice is gated by a Phase 2 spike against the corpus (ADR-015). If `@tiptap/markdown` can't meet guarantees 1–3, the fallback is a custom `prosemirror-markdown` serializer.

There are two Markdown parsers, `marked` (inside `@tiptap/markdown`) in TS and `pulldown-cmark` in Rust. They could disagree on edge cases. The Rust side only extracts plain text, links and attachment references, which is far less sensitive to that, and the shared fixture corpus enforces agreement on the parts that matter.

### 6.4 Autosave

- Debounced save 750 ms after the last change, a 5 s maximum delay during continuous typing, and an immediate flush on page switch, editor blur, window close and app quit (§8, quit handshake).
- At most one save in flight per page. Edits made meanwhile are saved right after it.
- Only real document changes mark the page edited. Opening a page, or switching the editor between editable and read-only, never writes it.
- On a save failure (disk full, permissions), the editor keeps its content, shows "Couldn't save — retrying" and retries with backoff (2, 5, then every 10 s). On a conflict it pauses and writes the buffer to `.forma/recovery/` at once. When the editor closes (page switch, quit) with text still unsaved, that text is written to recovery as well. The UI never shows "Saved" for content that wasn't persisted.
- Title edits go through `set_title`, which updates the frontmatter and renames the file or folder. They run when the title field has been idle for 600 ms, loses focus, or gets Enter, so a rename doesn't happen on every keystroke.

### 6.5 Page links

- On disk: a standard CommonMark link, `[Page Title](forma://page/<uuid>)`. Every Markdown tool renders it as readable linked text. Renaming or moving the target never breaks it, and no other files are rewritten. The label is refreshed to the target's current title whenever the *source* page is saved.
- Typing `[[` opens title autocomplete and inserts that link.
- **Read support for wikilinks.** A literal `[[Some Title]]` written in another editor is resolved by title. A unique match becomes a link, and an ambiguous or missing one shows as an unresolved link. The text is preserved exactly as written.
- Backlinks come from the index's `links` table and appear at the bottom of the page.
- Links to a trashed page render as "deleted page" and heal when the page is restored.

### 6.6 Attachments

- Pasted images (as bytes) and dropped files (as drop tokens, §2.1) go to Rust (`import_attachment(page_id, bytes | drop_token, name)`). If needed, Rust converts the page from a leaf to a bundle, writes `_assets/<unique-name>`, and returns the relative reference `_assets/<name>`.
- The webview loads attachments through a custom `forma-asset://localhost/<page-id>/<rel-path>` protocol. Rust resolves the page's directory, canonicalizes the path and rejects anything outside the workspace or behind symlinks.
- Remote images in documents (ADR-031):
  - `https:` images load automatically, which means opening the page contacts the image's server.
  - Plain `http:` images aren't loaded. They render as a placeholder showing the URL.
  - Either way, the Markdown is kept exactly as written.
- Binaries are never stored in SQLite.

---

## 7. Frontend architecture

- **React 19 + TypeScript 6**, Vite 8, Tailwind CSS 4 with CSS-variable design tokens, shadcn/ui (Radix primitives) and Lucide icons.
- **State.** Small zustand stores per feature: layout and panels, theme, the active page and editor session, tree expansion, search UI, and AI chat. The application never holds the whole workspace in React state. Tree children load per expanded node through IPC, document content loads only for the open page, and search results are paged.
- **Invalidation.** Stores subscribe to `workspace://changed` and refetch only the affected nodes and pages.
- **Large lists** (sidebar tree, collection tables, search results) are virtualized with `@tanstack/react-virtual`.
- **Layout.** Three panels: sidebar (visible by default), editor, and AI panel (hidden by default). The side panels have fixed pixel widths, resized with small custom handles (ADR-029). Widths and visibility persist in app settings. When the window is too narrow for a panel to sit beside an editor at least 420 px wide, that panel floats over the editor instead of disappearing, so a toggle always does what it says.
- **macOS window.** An overlay title bar with inset traffic lights and drag regions, native menus with standard shortcuts, and window-state persistence. Solid surfaces only; no glass effects.
- **Shortcuts.** App-level shortcuts are native menu accelerators. The menu sends a command ID to the webview, which runs it through one command registry (ADR-030). The webview doesn't handle those keys itself, so a shortcut can't fire twice.
- **Theming.** Tokens are defined for light and dark, plus "follow system". The window starts hidden. The frontend loads settings, applies the theme, renders, and then calls `app_ready` to show the window, so the first visible frame already has the right theme. If `app_ready` never arrives, the shell shows the window after 3 s anyway. For explicit Light or Dark, the shell also sets the native window appearance, which keeps the title bar and `prefers-color-scheme` consistent.
- **Browser preview (development only).** When `vite dev` is opened in an ordinary browser, a small harness stands in for the Rust side, with in-memory settings and keyboard-to-menu mapping, so the UI can be previewed without the app. Production builds strip it out.

---

## 8. IPC contract

| Mechanism | Use |
|---|---|
| `invoke` commands | Request and response: reads and mutations. Arguments use IDs, never paths. Errors are typed (`NotFound`, `Conflict{…}`, `Validation{…}`, `Io{…}`, `ReadOnly`, …) |
| Events | Broadcasts: `workspace://changed` (`{ content_changed, removed, tree_changed }`), `forma://menu` (a command ID), `forma://flush-before-quit`, and `index://status` from Phase 3. There is no `workspace://conflict` event (ADR-033) |
| Channels | Streams: AI token and tool events for one request, and index rebuild progress |

- Rust request and response types derive TypeScript definitions with **`ts-rs`**. The generated files go into `src/lib/ipc/bindings/` and are committed. CI fails if regenerating them produces a diff.
- `src/lib/ipc/` wraps `invoke` in typed functions. Components never call `invoke` directly. A contract test (`src/lib/ipc/contract.test.ts`) reads the Rust command signatures and fails if the client sends an argument name a command doesn't take.
- **Quit handshake.** Closing the window or quitting asks the webview to save first: the shell sends `forma://flush-before-quit`, the frontend flushes every open editor and calls `confirm_quit`, and the app exits. If no confirmation arrives within 5 seconds, it quits anyway and logs a warning. Text that couldn't be saved is kept as a recovery buffer (§6.4).
- Tauri capabilities allow-list exactly Forma's commands plus the dialog and opener plugins. No `fs` or `shell` plugin is granted to the webview.

The full command list is defined during each phase, starting from the Document Service API in [DATA_MODEL.md §10](DATA_MODEL.md#10-document-service-api-surface).

---

## 9. AI provider layer (`forma-ai`)

### 9.1 Abstraction

As built in Phase 4 (ADR-039). One `Provider` type serves every configured provider and picks the wire protocol from its kind. Each protocol is a module with a request builder and a pure stream decoder:

```rust
impl Provider<'_> {
    async fn list_models(&self) -> Result<Vec<ModelInfo>, AiError>;
    async fn capabilities(&self, model: &str) -> Result<ModelCapabilities, AiError>;
    async fn stream_chat(&self, req: &ChatRequest, cancel: &CancellationToken,
                         on_event: &mut (dyn FnMut(ChatEvent) + Send)) -> Result<(), AiError>;
}

trait Decoder { fn decode(&mut self, frame: Frame) -> Result<Vec<ChatEvent>, AiError>; … }

struct ChatRequest { model, system, messages: Vec<Message>, tools: Vec<ToolSpec>,
                     max_output_tokens, temperature, context_window }
enum ChatEvent { TextDelta(String), ToolCallStart{id, name}, ToolCallArgsDelta{id, delta},
                 ToolCallEnd{id}, Usage{input, output}, Done(StopReason) }
struct ModelCapabilities { tool_calling: Tri, context_window: Option<u32>, max_output_tokens: Option<u32> }
struct AiError { kind: AiErrorKind, message, status: Option<u16>, retry_after_secs: Option<u32> }
// AiErrorKind: Disabled, NotConfigured, Auth, Unreachable, Tls, Timeout, RateLimited,
//              ModelNotFound, ContextOverflow, Provider, Interrupted, InvalidResponse, Cancelled
```

`Tri` is `Yes`, `No` or `Unknown`. Forma never assumes that every model has the same capabilities. `AiService` sits above `Provider`: it holds the keys read this session, the model details it has fetched and the chats that are running, and it decides whether a request may be sent at all (§9.3).

### 9.2 Adapters

Two wire protocols plus provider presets keep the code small:

| Preset | Wire protocol | Auth | Model list and capabilities |
|---|---|---|---|
| OpenAI | OpenAI Responses API (SSE), with `store: false` (ADR-038) | API key (Keychain) | `/v1/models`, filtered to models that can chat. Capabilities `Unknown`: the list gives none |
| Anthropic | Anthropic Messages API (SSE), version `2026-10-21`, no temperature (ADR-038) | API key (Keychain) | `/v1/models` (paged), `/v1/models/{id}` for the context window and output limit |
| OpenAI-compatible | Chat Completions | Optional key | `/v1/models`. Capabilities `Unknown` unless the user overrides them |
| Ollama | Native `/api/chat` (NDJSON stream), always with `num_ctx` | None (localhost) | `/api/tags`, `/api/show` (tool support, context length) |
| LM Studio | Chat Completions at `localhost:1234/v1` | None | `/v1/models`, LM Studio REST model info where available |

Ollama uses its native API because the OpenAI-compatible endpoint can't set `num_ctx`, so long contexts would be truncated silently.

- Model lists come from provider APIs, not hard-coded lists. The user can type a model ID manually.
- Streaming uses SSE or NDJSON parsed in Rust and forwarded through a Tauri Channel. Cancellation propagates via a `CancellationToken`, which aborts the HTTP request.
- **Connection test:** list models, then ask the chosen model to "Reply with the word OK." (at most 16 tokens). The result names the exact failure: authentication, endpoint unreachable, model missing, TLS, timeout or rate limit.
- **Timeouts:** 10 s to connect, 300 s between chunks of an answer (a local model can take that long to load), 30 s for model lists and details.

### 9.3 Credentials and privacy

- API keys are stored in the **macOS Keychain** (`keyring-core` with `apple-native-keyring-store`, behind a `SecretStore` trait). Keys never appear in `settings.json`, the workspace, logs or the webview. Settings only show whether a key is stored, with Replace and Remove actions.
- Only official authentication: provider API keys, or no auth for local servers. A consumer chat subscription (ChatGPT Plus or Pro, Claude Pro or Max) does not grant API access, and session-token extraction is never implemented.
- **Cloud transmission notice.** The first use of a provider whose address isn't on this Mac (not a loopback address) requires an explicit acknowledgement, recorded per provider and cleared when its host changes. Rust refuses the request without it. Every message shows where it went and to which model. Local providers say "stays on this Mac". Only context the request needs is sent (§11): what the user typed, and, from Phase 5, the attached page or selection and the passages found for the message. The agreement's text says so; agreements given under Phase 4's wording ("never your notes") don't carry over (ADR-042). With "Search notes" off and nothing attached, only what the user typed is sent.
- **Gate in Rust.** `AiService` refuses to send unless AI is on, a provider and model are chosen, a required key is stored, and the acknowledgement above is given. Each request is packed to fit the context window (§11); one whose new message alone can't fit is refused instead of being cut by the provider.
- **Keys at runtime.** Read once per session from the Keychain on a blocking thread (macOS may prompt) and kept in Rust memory. Sent with `Authorization: Bearer`, which the HTTP client marks sensitive. Any copy of the key in a provider's error message is replaced before the message leaves Rust, and AI failures are logged by kind and status only.

### 9.4 IPC commands (Phases 4–5)

`ai_set_enabled`, `ai_add_provider`, `ai_update_provider`, `ai_remove_provider` (also deletes the key), `ai_set_active_provider`, `ai_set_api_key`, `ai_remove_api_key`, `ai_acknowledge_cloud`, `ai_list_models`, `ai_model_capabilities`, `ai_test_connection`, `ai_chat` and `ai_cancel`. The settings commands return the saved settings; none returns a key.

`ai_chat(request_id, input, on_event)` takes the conversation and an `AskContext` (page ID to attach, selected text with its page ID, the cursor's block text, whether to search the notes, and where source numbering starts). It never takes a path: notes are read in Rust, through the read-only view. It streams `ChatStreamEvent`s over a Channel: `context` (what went in), `sources`, `tool` (a lookup the model made), `text`, `usage`, `done` and `failed`.

---

## 10. AI agent (`forma-ai::agent`)

### 10.1 Modes and capability separation

| Mode | Tools exposed to the model | Can change data? |
|---|---|---|
| Ask | Read tools only | No |
| Act | Read tools and proposal tools | Only by creating a **ChangeSet proposal**. Applying it is a separate user command the model can't reach |

The agent runtime holds a **read-only handle** to the Document Service plus a ChangeSet builder. It has no reference to any mutation API, so authorization is enforced by construction, not by prompt instructions. Applying a change is the `apply_changeset` IPC command, which only the approval UI calls.

Act mode (called **Edit** in the panel) needs a model with tool calling. When tool support is `No` or `Unknown`, Act mode is unavailable and the UI says why. Ask mode still works with any model through pre-retrieval (§11).

**As built in Phase 5 (Ask mode, ADR-042).** The read-only handle is `forma_core::workspace::read::ReadView`: it has no method that writes. `Workspace` also has a few `&self` methods that write (recovery buffers, snapshots), so the type matters, not just the `&`. `forma_ai::knowledge::Library` wraps the session's workspace slot and hands out only a `ReadView`, briefly, per read. Each Ask tool is a `ReadTool` whose `run` takes that view and its arguments, and a test pins the registry to that type. Tool use follows the provider's report or the user's setting (Settings → AI → "Look things up in your notes"). With "Search notes" turned off in the panel, no tools are offered.

### 10.2 Tool registry

Each tool has a name, a description, an input JSON Schema (generated from Rust types), an output type, a kind (`Read` or `Propose`), validation, and limits on result size and iteration count.

| Tool | Kind | Notes |
|---|---|---|
| `search_pages(query, limit?)` | Read | Passage retrieval (§11). Returns up to 10 passages (5 by default) as numbered `<source>` blocks with their page IDs |
| `read_page(page_id, part?)` | Read | Body Markdown as a numbered source. Long pages come in parts of about 2,400 tokens, cut between passages |
| `list_pages(parent_id?)` | Read | Children: ID, title, whether it has subpages, whether it's a collection (at most 100) |
| `get_page_metadata(page_id)` | Read | Title, path, parent, tags, properties, dates, collection, body revision |
| `create_page(title, parent_id?, content?)` | Propose | Returns `new-N`, usable as a page ID in later proposals |
| `edit_page(page_id, edits \| replace_all_body)` | Propose | `edits` = exact `{old, new}` replacements (each `old` must match exactly once). The body revision is captured when proposed, not passed |
| `move_page(page_id, new_parent_id?)` | Propose | Cycle check, counting pages proposed in the set |
| `create_collection(title, parent_id?, properties)` | Propose | Properties: name, type, options |
| `update_page_properties(page_id, set?, unset?)` | Propose | By property name, typed against the schema (option names case-insensitive) |
| `link_pages(source_id, target_id, text?)` | Propose | An edit that turns `text` (once in the page) into a page link, or adds the link as a last line. Applying writes the target's ID if it isn't in its file |

- Tools accept **page IDs only, never paths**. Every ID is resolved through the index and the Document Service, which is how workspace boundaries are enforced.
- Invalid arguments come back to the model as structured tool errors, so it can correct them. The runtime never panics or partially applies a change.
- Limits: at most 12 tool rounds per request, tool results truncated to a token budget, and a wall-clock timeout. All are cancellable. Ask mode uses 6 rounds and 10 minutes, then one round that must answer (the tools stay defined, with `tool_choice: none`); each result is cut to 3,000 tokens and to the room left in the window.

### 10.3 Proposal → preview → approval → apply → undo

```
User request → context retrieval → model ⇄ read tools → proposal tools → ChangeSet (in memory)
  → Preview: pages to create / edit / move, collection & property changes,
             per-page Markdown diff (word-level, `similar`)
  → User: approve all · approve selected · reject     (rejection changes nothing on disk)
  → apply_changeset(id, selected_ops):
       journal intent → protected history snapshots → per-op base_rev check
       → apply via DocumentService batch (all-or-nothing; rollback from snapshots on failure)
       → write .forma/operations/<op-id>.json {ops, before/after revs, snapshot refs}
  → Result summary with links to affected pages · "Undo" button
```

- **Conflicts.** If any selected operation's `base_rev` no longer matches, none of the selected operations are applied. The preview marks the conflicting ones and offers to refresh the proposal against the current content.
- **Undo** restores each affected page from its snapshot, but only if the page is still at the revision the operation left it in. Created pages are moved to the trash and moved pages are moved back. An undo is itself a recorded operation, so it can be redone.

**As built in Phase 6 (ADR-043).**

- **Edit mode** in the panel (Ask | Edit) offers the read tools and the six proposal tools, for models that can call tools. The model's proposals build a `forma_core::workspace::changeset::ChangeSet` in memory; at the end of the answer, Rust keeps the set (the last 20) and sends its preview. `apply_changeset(changeset_id, change_ids)`, `reject_changeset`, `list_operations` and `undo_operation` are the commands; only the review dialog and the history dialog call them.
- **Transactions** (`workspace/tx.rs`). Applying and undoing open a transaction: every file action the Document Service takes is added to a journal in `.forma/journal/` (with a snapshot of any file it overwrites) before it's done. A failure undoes the recorded actions newest first; after a crash, the next start does the same. Each step back checks the file first and leaves one changed since alone. The Document Service's own methods (create, move, trash, restore, collections) are transaction-aware, so a set uses the same code as the UI.
- **Undo is all or nothing.** If any page isn't as the operation left it, nothing is undone, and the conflicts are listed (not page by page, as first planned: a half-reverted set would match no record). A created page goes to the trash; a page it had been put in keeps its folder form.
- **Before applying or undoing,** the panel saves text being typed, so a page edited since shows up as a conflict.
- **History:** the panel's "AI changes" dialog lists the recorded operations, newest first, with Undo, or Redo for an undo.

### 10.4 Prompt-injection posture

Retrieved documents are untrusted. The defenses, from strongest to weakest:

1. **Structural.** The model can't write without explicit user approval of a visible diff, and it can't reach files outside the workspace, the shell or the network.
2. **Isolation of content.** Document text is passed inside delimited, labeled data blocks, and the system prompt states that document content is data, not instructions.
3. **Output hardening.** Chat output is rendered as sanitized Markdown, with no raw HTML, no remote images (blocking the "image URL exfiltration" pattern) and no `javascript:` links. External links show their destination and need a click. Notes do load `https:` images (ADR-031), so the change preview shows images as URLs rather than loading them, and flags every new external image a proposal would add before the user approves it.
4. **Transparency.** The preview shows the full proposed content, so injected instructions that lead to a strange proposal are visible before anything changes.

The Phase 6 test suite includes adversarial documents.

---

## 11. Knowledge retrieval and context management

As built in Phase 5 (ADR-041, ADR-042).

- **Passages.** Each page body is cut at headings and between blocks into passages of about 250–400 tokens (`forma_core::passages`). A passage keeps its heading path, its byte range in the body, its Markdown and its plain text. They're indexed with the page, in `passages` and `passages_fts` (stemmed English words).
- **Retriever.** `Retriever::retrieve(query, k, filter) -> Vec<RetrievedPassage>` has one implementation, on `ReadView`: BM25 over the passage words (any word may match; English stop words dropped; title and headings weighted up), plus, for CJK text, pages found through the page trigram table whose passages are ranked by how many of the question's three-character pieces they share. The two rankings are merged by reciprocal rank, with at most three passages per page. An embedding or hybrid retriever could implement the same trait later.
- **Pre-retrieval** works for every model: with "Search notes" on, the new message is the query, and the top passages (up to 8) go with it as numbered sources. Tool-capable models can also call `search_pages`, `read_page`, `list_pages` and `get_page_metadata` themselves (§10.2).
- **Context packing** fills the window less the answer's reserve (the maximum output setting, else 1,024 tokens, at most half the window), in order: the system prompt and the new message (always; if they don't fit, the request is refused), the selection (up to 30% of what's left), the attached page (up to 60%; if it's longer, the passages nearest the cursor's block, in page order, with gaps marked "[…]"), retrieved passages (up to 60% of what's left), then earlier turns, newest first. Older turns are dropped first. Tokens are estimated conservatively (`forma_core::tokens`): about 4 characters per token for Latin text and 1 per character for CJK. The window is the user's override, else the model's reported window, else 8,192 tokens. Rust reports what went in, and the panel shows it under the message.
- **Sources.** Each piece of the notes sent is a `<source key="S#" kind="selection|page|passage" page="…" page_id="…" section="…">` block in the user's turn. Keys continue across the conversation (the panel sends where numbering starts). A source gets its key only if its text is actually sent. Source text can't close its block.
- **Citations.** The model is asked to cite `[S#]`. The panel receives the sources from Rust and checks each citation in an answer against the sources of that answer: a match is a button that opens the page and highlights the passage (found by its text, briefly, as a decoration that doesn't touch the file); any other key is shown as an unverified citation and links nowhere. Under the answer, the panel lists the cited sources, the provided ones it didn't cite, and the unverified keys.
- **Missing information.** The system prompt asks the model to say when the sources don't contain the answer, and to mark general knowledge as not from the notes. When search finds nothing, the request says so ("Nothing in my notes matched"), and the panel shows "No notes matched."
- **Actions.** "Ask about this page" and "Summarize page" (page tree menu, and the panel when a page is open) attach the page; "Ask AI" and "Explain" in the selection menu attach the selected text. Summarize and Explain also send a short request. They appear only when AI is on.

---

## 12. Security summary

| Threat | Mitigation |
|---|---|
| Path traversal through IPC or tools | IDs instead of paths. Paths computed in Rust, canonicalized, with a prefix check against the workspace root. No symlink following. Dropped files are referenced by single-use tokens, and folders come from a dialog Rust opens itself |
| Malicious Markdown, HTML or XSS in documents | No raw HTML rendering, strict CSP, `javascript:` and `data:` URLs filtered from links, KaTeX in `trust: false` mode |
| Data exfiltration through remote resources | The webview has no network access except `https:` images in notes (ADR-031). AI chat output and AI change previews never load remote images, and previews flag newly added external images. AI calls go only through Rust |
| Credential leakage | Keychain only. Keys never cross IPC to the webview and are redacted from logs and errors. `.gitignore` covers env and secret files |
| Prompt injection driving writes | Capability separation, proposals only, user-only apply, diff preview, per-op revision checks |
| Unauthorized workspace access | One workspace root per session. The asset protocol and tools are scoped to it |
| Concurrent writers | Single instance, workspace `flock`, single-writer service, revision checks |
| Supply chain | Minimal dependencies, lockfiles committed, `cargo audit` and `npm audit` in CI, third-party GitHub Actions pinned to commit SHAs, no Tiptap Pro or other non-OSI dependencies |
| Malicious backup archives (zip slip, links, zip bombs) | Restore checks every entry before writing: relative names only, normal components, no symbolic links, at most 500,000 entries and 20 GB, each entry read up to its declared size. It unpacks into a staging folder and never over existing files (ADR-044) |
| Bug reports leaking notes | "Copy diagnostics" holds no note text, titles, keys or server addresses; log lines have quoted strings and paths replaced. Nothing is sent; the user pastes it |

---

## 13. Future collaboration extension points (not implemented)

v0.1 has no accounts, sync, CRDT or multiplayer features. These existing properties keep that door open:

1. **Stable UUIDs** for pages, collections, properties and options. A future sync layer can key everything by ID.
2. **A single mutation API.** All changes go through typed Document Service operations, the same ops the ChangeSet system uses. A sync engine can record, replay or transmit them as an operation log.
3. **Revisions as content hashes** support cheap "has this changed?" checks and three-way merges against history snapshots.
4. **The editor is separate from persistence.** The editor exchanges Markdown and typed patches with the service. A future CRDT model (for example Yjs bound to ProseMirror) could replace the in-memory document without changing the storage format.
5. **Explicit collection schemas** with ID-keyed properties and options allow a future server to validate them.
6. **History and the operation log** are already the raw material for conflict resolution UIs.

Expected future work, intentionally deferred: identity and authorship metadata, a sync protocol, CRDT document state stored next to the Markdown files, per-page permissions, and real-time presence.

---

## 14. Risk register

| # | Risk | Likelihood / Impact | Mitigation | Phase |
|---|---|---|---|---|
| R1 | `@tiptap/markdown` round-trip fidelity is insufficient (tables, nested lists, math, escaping) | Medium / High | Round-trip corpus spike at the start of Phase 2. Fallback to a custom `prosemirror-markdown` serializer. Raw-block preservation, no-write-without-edit and first-write snapshots | 2 |
| R2 | External edits race with Forma writes (TOCTOU) | Low / High | Hash checks immediately before `rename`, pre-overwrite snapshots, documented residual window | 2 |
| R3 | Sync tools (iCloud, Dropbox) create conflict copies, evict files or reorder events | Medium / Medium | Duplicate-ID handling, an index outside the workspace, event-as-hint reconciliation. v0.1 documents synced folders as "not officially supported" | 2, 7 |
| R4 | macOS path limits (`PATH_MAX` 1024 bytes) with deep trees and long titles | Low / Medium | Filename stems capped at 100 bytes, a depth warning, and an error message that points at the problem before any write is attempted | 2 |
| R5 | APFS is case- and normalization-insensitive by default, so sibling names collide | Medium / Low | NFC normalization and case-folded uniqueness checks with numeric suffixes | 2 |
| R6 | CJK full-text search quality with `unicode61` | High (for CJK users) / Medium | **Mitigated in Phase 3** (ADR-036): trigram table with `LIKE`, plus a bounded scan for 1–2-character queries. No dictionary segmentation, so results are substring matches, not word matches | 3 |
| R7 | Keychain access prompts during development (ad-hoc signatures change between builds) | High in dev / Low | Documented. Keys are read once per session. **Phase 7:** releases are ad-hoc signed too, so every update asks again (USER_GUIDE, CONTRIBUTING). A Developer ID would give a stable identity | 4, 7 |
| R8 | Provider API drift (OpenAI Chat Completions vs Responses, Anthropic versions, Ollama endpoints) | Medium / Medium | **Checked in Phase 4** (ADR-038). Protocols isolated in modules with stream-fixture tests; an ignored live test checks the cloud APIs' auth and TLS | 4 |
| R9 | Local models silently truncate context (Ollama's `num_ctx` default) | High / Medium | **Phase 4:** native Ollama API with an explicit `num_ctx`. **Phase 5:** every request is packed to fit that window, and tool rounds stop before it fills | 4–5 |
| R10 | Prompt injection through retrieved notes | High / Medium | §10.4. The structural guarantee doesn't depend on model behavior. **Phase 5:** sources are delimited data blocks that can't close themselves, the system prompt says to ignore instructions in them, and the assistant has only a read-only view. **Phase 6:** proposals are data; only the review dialog applies them; tests run a model that obeys an injected note and check that nothing changes | 5–6 |
| R11 | XSS in the webview defeats the approval boundary | Low / High | No raw HTML, strict CSP, sanitized link schemes, security tests | 2, 6 |
| R12 | Lazy ID persistence leaves links pointing at index-only IDs | Low / Medium | Invariant: `ensure_persisted_id` runs before any durable reference is created, covered by tests | 2–3 |
| R13 | Distribution: unsigned or un-notarized DMG is blocked by Gatekeeper | High / Medium | **Phase 7:** ad-hoc signed, not notarized; `spctl` rejects it, as expected. The user guide explains how to allow it once, and every release states the signing status (ADR-045). Notarization needs an Apple Developer ID (user decision) | 7 |
| R14 | Intel support: `x86_64-apple-darwin` is a Tier 2 Rust target, and Intel CI runners are scarce | Medium / Low | **Phase 7:** the release is a universal binary cross-compiled on Apple silicon. Its x86_64 slice was launched under Rosetta 2 (window shown, idle, clean quit); it hasn't run on an Intel Mac | 7 |
| R15 | The "Forma" name collides with existing products (for example Autodesk Forma) | **High** / Medium | **Phase 7 quick web search (not a legal clearance):** "Forma – Notes & Tasks", a Mac notes and tasks app; Autodesk's "AUTODESK FORMA" application in class 42 (software); an iOS PDF app named Forma. A professional trademark search, or a new name, before any public release (user decision). The bundle ID `app.forma.desktop` follows the name and can't change later without moving settings and Keychain items | 7 |
| R16 | TypeScript 7 ecosystem incompatibility | Avoided | Pinned to TypeScript 6.0 (ADR-016). Revisit when typescript-eslint, editors and other tools support TS 7 | — |
| R17 | Automatically loaded `https:` images in notes reveal reading activity (tracking pixels), and an approved AI edit could add an exfiltration URL | Medium / Medium | User-accepted tradeoff (ADR-031). `http:` isn't loaded. AI chat output never loads remote images. Change previews show image URLs and flag new external images. A setting to turn remote images off can be added if needed | 2, 6 |
