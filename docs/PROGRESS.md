# Forma Development Progress

## Current Phase

Phase 3: Collections, Links, and Search (approved 2026-11-02). **Implemented and verified; waiting for the user's approval to proceed to Phase 4.**

## Status

Phase 3 is complete as far as automated, performance and runtime checks can show. The manual checks in the built app (below) are for the user.

### Phase 3 journal

- **SQLite index (ADR-035).** A cache in Application Support, loaded when a workspace opens so only changed files are read, and updated in one transaction after every rescan and every write. Settings → Data → "Rebuild search index" builds a side database and swaps it in. A test checks that a rebuilt index equals the incrementally maintained one, row for row.
- **Search (ADR-036).** ⌘K searches titles, text, tags and property values with BM25 ranking, prefixes, snippets and highlights. Chinese, Japanese and Korean queries use a trigram table: the default tokenizer couldn't find `注意力` inside a sentence. ⌘P matches titles and lists the pages opened this session.
- **Links.** Page links are editor nodes that show the target's current title, and the label in the file is refreshed when the page is saved (no other file is rewritten). `[[` opens title search, and persists the target's ID before inserting the link. Wikilinks resolve by path, title, then file name; an ambiguous or missing one is marked. Links to a trashed page say so and heal on restore. Backlinks appear under each page. The editor and the index read the same link fixtures (`tests/fixtures/links/`).
- **Collections.** Create, convert, or turn back into a regular page. Six property types, with the DATA_MODEL §6.3 reading rules: invalid values are shown and never rewritten unless edited; an empty value is an absent key. Renaming a property or option rewrites the entries as one journaled batch, rolled back on the next start if interrupted. Table view (rows drawn only when on screen, editing in place, column resize and hide, new entries) and list view. Sort and AND filters, saved in `.forma/views.json`. Entry pages show their properties under the title. Sidebar per ADR-037.
- **Bugs found and fixed during the phase:**
  1. *Forma's own saves didn't reach the index* (it only updated on rescans). Every write now indexes the page.
  2. *Backlinks took 3.4 s* at 5,000 pages (every wikilink re-scanned every title). A lookup table and a name pre-filter: 6.6 ms.
  3. *Focus could scroll the whole window sideways* to reveal the hidden AI panel (`overflow: hidden` still scrolls programmatically). The shell now uses `overflow: clip`. This dated from Phase 1.
  4. *A schema version reached the UI typed as `bigint`*, though JSON delivers a number. Fixed in the binding.
  5. *Numbers showed with thousands separators* ("2,017" for a year). They show as written.
- **Design decisions:** ADR-035, ADR-036, ADR-037 (above).

### Phase 2 journal

- **Audit items F5 and F7 fixed.** Atomic writes keep permissions and extended attributes. Logs go to `~/Library/Logs/app.forma.desktop/forma.log`.
- **Markdown gate passed (ADR-034).** `@tiptap/markdown` plus a Forma rules layer. Eight library problems were found and fixed (code fences, line-start escaping, HTML entities, underscores, autolinks, hard breaks, table spacing and alignment rows, currency read as math). 23 fixtures, byte-identical for canonical input.
- **Rust core (`forma-core`).** Frontmatter (surgical edits), naming, scan (in-memory index, ADR-032), history, trash, recovery, relative-link rewriting, watcher, and the Document Service.
- **Shell.** 32 commands, the `forma-asset://` protocol (refuses paths outside the workspace, inside `.forma`, or reached through symlinks), the quit handshake, a native folder picker, and a warning before opening a folder with more than 10,000 entries.
- **UI.** Welcome screen, page tree (lazy children, rename, drag and drop, context and ⋯ menus, Move to…, duplicate, favorites, trash with restore and confirmed delete), breadcrumbs, title field, Tiptap editor (slash menu, formatting bubble, KaTeX, tables, task lists, images by paste and drop), autosave with status, conflict banner, recovery dialog, Settings → Workspace.
- **Bugs found and fixed by the Phase 2 tests and checks:**
  1. *Opening a page wrote it.* Tiptap's `setEditable()` emits an update by default, which looked like an edit. Now only document changes mark a page edited (architecture rule 14).
  2. *Edits were silently dropped in development builds.* React runs effects twice in development; the save controller was disposed and reused. Each mount now gets its own controller. Production builds weren't affected.
  3. *Deleting an open page outside Forma lost unsaved edits.* The page now stays open with a banner (restore with my text, or discard), as ARCHITECTURE §4.9 specifies.
  4. *A failed save on page switch or quit lost the text.* It's now kept as a recovery buffer.
  5. *An endless re-render on the welcome screen* (a store selector returned a new array each time).
  6. *Duplicate React keys* made a page show several title fields after a rename.
  7. *A copied page got a new ID on every outside edit*, and its notice repeated. It now keeps the first one.
  8. *Notices from rescans were never shown.* They're now fetched after each change.
  9. *A new page didn't put the cursor in its title.*
  10. *Tables were saved with an extra blank line* and a misaligned alignment row.
- **Design decisions:** ADR-032 (in-memory index until Phase 3), ADR-033 (body revision for editor saves; approved by the user), ADR-034 (Markdown gate outcome).
- **Deferred, with reasons:** the `journal` module (first used by collections in Phase 3; no speculative modules), the `workspace://conflict` event (replaced, ADR-033).

## Completed Phases

- **Phase 0: Architecture and Project Planning.** Approved 2026-11-02 (commit `0b64019`).
- **Phase 1: macOS Application Foundation**, with the Phase 1.1 audit fixes. Commits `fd1dc6b` and `c2145e7`; design decisions recorded in `a90c3c6`.
- **Phase 2: Document System and Editor.** Approved 2026-11-02 (commits `52c5d1e`, `7cbe4c7`).

## Implemented Features

### Phase 2 (2026-11-02)

See the Phase 2 journal above. Data-integrity behavior in brief:

- Every write is atomic (staged in `.forma/tmp`, `F_FULLFSYNC`, rename, directory fsync) and keeps permissions and Finder tags.
- Opening a workspace or a page writes nothing outside `.forma`. Verified in the release app: no workspace file changed.
- A save checks the body revision first. If the body changed elsewhere, nothing is written and the conflict banner appears; the editor's text goes to `.forma/recovery/` at once.
- Content that came from outside Forma is snapshotted to `.forma/history/` before Forma first overwrites it. Snapshots from the last 30 days are kept; older ones are thinned to one per day.
- Trash keeps whole subtrees and restores them to their parent, or to the top level if the parent is gone.
- Quitting saves first (quit handshake), and anything that couldn't be saved is offered back on the next launch.

### Phase 1 (2026-11-02)

- **Toolchain:** Rust 1.99.1 (rustup, plus the `x86_64-apple-darwin` target) and Node 26.13.0 LTS with npm 11.20 (Homebrew).
- **Project scaffold:**
  - Cargo workspace: `crates/forma-core` and the `src-tauri` shell.
  - Frontend: Vite 8, React 19, TypeScript 6, Tailwind 4, shadcn/ui primitives (button, dialog, tooltip, toggle group) and Biome.
- **`forma-core`:**
  - Atomic file writes: temp file, `F_FULLFSYNC`, rename, then fsync of the directory, plus cleanup of stale temp files.
  - App settings store (`settings.json` in `~/Library/Application Support/app.forma.desktop/`):
    - Defaults are used when the file is missing.
    - A corrupt file is moved aside as `settings.json.corrupt-<ts>`, with a notice to the user.
    - Widths are clamped.
    - Unknown keys are preserved and never sent to the webview.
    - A file from a newer format version is read-only.
  - TypeScript bindings generated with ts-rs.
- **Tauri shell:**
  - Overlay title bar with inset traffic lights. The window stays hidden until the first themed frame (`app_ready`), with a 3 s fallback.
  - Native window appearance and background follow the theme.
  - Native menus: Forma, Edit, View, Go, Window.
  - Single instance and window-state persistence.
  - Strict CSP. Every app command needs an explicit capability permission.
- **UI:**
  - Three-panel layout: sidebar, editor, and AI panel (hidden by default).
  - Smooth collapse, pixel-width resize handles (pointer, keyboard, double-click to reset), and overlay mode for narrow windows.
  - Light, dark and system themes from design tokens.
  - Honest empty states for the sidebar, editor, AI panel, ⌘K search and ⌘P quick open.
  - Settings dialog: Appearance works. Workspace, AI and Data explain when they arrive and contain no controls.
  - A notice bar for settings problems.
- **Shortcuts (native menu accelerators, ADR-030):** ⌘⇧L sidebar, ⌘⇧A AI panel, ⌘K search, ⌘P quick open, ⌘, settings. Esc closes dialogs. Standard macOS items (⌘Q, ⌘H, ⌘M, ⌘W, Edit menu, full screen) are untouched.
- **Development-only browser preview:** `npm run dev` opened in a browser uses an in-memory stand-in for the backend. Verified to be absent from the production bundle.
- **New decisions:** ADR-029 (custom resize handles) and ADR-030 (menu-accelerator shortcuts).

### Phase 1.1: audit fixes (2026-11-02)

- **F1, title-bar double-click zoom.** The capability granted `core:window:allow-toggle-maximize`, but Tauri's drag-region script needs `core:window:allow-internal-toggle-maximize`. The grant is corrected, and a test guards both title-bar permissions (`src-tauri/src/window.rs`).
- **F2, bundle signature.** The bundle was only linker-signed and failed `codesign --verify --strict`. With `bundle.macOS.signingIdentity: "-"`, Tauri now ad-hoc signs the whole bundle (with hardened runtime). Strict verification passes. It isn't notarized, so Gatekeeper on other Macs still warns.
- **F3, settings startup panic.** If the settings folder can't be used, the app now opens on read-only defaults and explains why (`SettingsStore::load_or_fallback` / `unavailable`) instead of panicking during setup.
- **F4, native theme while read-only.** A new `apply_window_theme` command changes only the native window appearance. Read-only mode uses it, so the title bar follows the chosen theme. The IPC snapshot's `read_only: bool` became `read_only_reason: string | null`, so the UI can say why settings are read-only.

### Phase 0

The architecture documents and CLAUDE.md. See the git history.

## Tests

Run on 2026-11-03 at the end of Phase 3 (`npm run check` exits 0):

| Command | Result |
|---|---|
| `npm run typecheck` (app and Vite config) | Pass |
| `npx biome check .` | Pass, 100 files, 0 diagnostics |
| `npx vitest run` | 23 files, **155 tests passed** |
| `cargo fmt --all --check` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |
| `cargo test --workspace` | **143 passed**: 140 in `forma-core` (including 37 workspace integration tests against real temporary folders) and 3 in the shell |
| `npm audit` | 0 vulnerabilities |
| `npm run tauri -- build --bundles app` | Pass. `Forma.app` is 10.43 MiB, arm64. The JS bundle is 1.35 MB (413 KB gzipped) |
| `codesign --verify --deep --strict Forma.app` | Pass (ad-hoc, hardened runtime). Not notarized |

New in Phase 3: index (word, prefix, substring and CJK search; rebuild equivalence; cache across restarts; outside edits while closed), links (renames and moves, wikilink resolution and ambiguity, trash and restore, parity fixtures on both sides), collections (schema validation, the coercion table, value writing, renames and their refusal, crash rollback, turning back into a page, views, newer schemas read-only), and in the UI: search keyboard handling and stale answers, `[[` link insertion (and no link when the ID can't be saved), page link titles and states, backlinks, table editing, option creation, filters, list view, read-only collections, the rebuild button.

**Performance** (5,000 synthetic pages from `node scripts/generate-workspace.mjs sandbox-workspaces/perf 5000`, 20 MB of Markdown, measured with `cargo run --release -p forma-core --example measure -- sandbox-workspaces/perf`; best of several runs):

| Operation | Time |
|---|---|
| Open, no index cache (cold) | 569–618 ms |
| Open, with the cache | 29 ms |
| List a folder's pages (the tree) | 0.03–0.04 ms |
| Search: word, prefix, three words | 3.6, 3.7, 4.8 ms |
| Search: `tention` (inside a word), `蛋白质`, `知识` | 0.24, 0.30, 0.27 ms |
| Quick open | 1.3 ms |
| Backlinks | 6.6 ms (3.4 s before the fix) |
| Load a 500-entry collection | 1.7 ms |
| Rescan with nothing changed | 18 ms |
| Rebuild the index | 574 ms |
| Index size on disk | 34.8 MB |

Runtime checks on the release app (2026-11-03): with a generated 300-page workspace, the window appeared in 0.84 s and the quit handshake took 0.20 s. Every one of the 301 files was unchanged afterwards, the workspace held no database files, and the index in Application Support had 301 pages, 653 links and 301 full-text rows. The test index and workspace were deleted afterwards.

Checked in the development browser preview, not the native app: page links and wikilinks with live titles and navigation, ⌘K results with path and highlighted snippet, the collection table (option picker, values written), the list view, an entry page's properties, and the sidebar's collection icon.

**Unverified in the native app** (no screenshots or input in this session): every interaction inside the window, as in Phase 2.

### Phase 2 test record

What the frontend tests cover: the Markdown corpus, the save controller, the page view (autosave arguments, no write on open, conflict and "use theirs", deleted-while-editing and restore, StrictMode double effects, title focus and rename), the page tree (open, expand, rename, Move to…), the workspace store, image source rules, the slash menu filter, and an **IPC contract test** that compares every client call's argument names with the Rust command signatures (it caught a mutation in a check run).

Runtime checks on the release app (2026-11-02):

- **Launch:** the window became visible in 0.46–0.83 s (4 runs).
- **Quit handshake:** the app exited 0.17–0.19 s after a quit request (4 runs), with no "didn't confirm saving" warning in the log. So the shell → webview → `confirm_quit` round trip works under the production CSP and capabilities.
- **Opening a real workspace writes nothing:** a sandbox workspace (frontmatter with a comment and an unknown key, a file without frontmatter, a wikilink, raw HTML, a table and an attachment) was opened by the release app. A file was then edited from outside while the app was running. Afterwards, every file's hash was unchanged except the outside edit, and `.forma/` held only `history lock recovery tmp trash workspace.json`.
- **macOS privacy prompts:** opening a workspace inside Desktop or Documents makes macOS ask for access the first time, and Forma waits until the user answers. Each ad-hoc-signed build counts as a new app, so development builds ask again. Outside protected folders the workspace opened at once (4 of 4 runs). A declined prompt now produces a message pointing to System Settings → Privacy & Security → Files and Folders (unit-tested; the native prompt itself is **Unverified**).

Checked in the development browser preview, not the native app: the editor's rendering in light and dark themes, the slash menu turning a line into a heading, the bubble menu's italic button, the new-page flow (title, then Enter, then body), autosave reaching the in-memory backend, the Trash dialog's empty state, and Settings → Workspace.

**Unverified in the native app** (this session can't take screenshots of it or send it input): every interaction inside the window, including typing, drag and drop in the tree, pasting or dropping images, the conflict banner after an outside edit, and the large-folder warning dialog.

## Manual Verification Steps (for the user)

Use a test folder, not real notes. Open `target/release/bundle/macos/Forma.app`.

**Phase 3**
1. **Search:** write a few pages, then ⌘K and search for a word in the body, a word start, and a Chinese word of two characters. Edit a page and search again: results follow the edit.
2. **Quick open:** ⌘P lists recently opened pages; typing filters by title.
3. **Links:** in a page, type `[[`, pick a page. Rename and move the target: the link shows the new title and still opens it. Trash the target: the link is struck through; restore it: the link is back. The target's backlinks list shows the source.
4. **Collections:** ⋯ → Turn into a collection. Add properties of each type with "+", add entries with "New", edit values in the table, sort and filter, switch to List. Quit and reopen: everything is kept. Open an entry: its properties are under the title. Open an entry's `.md` file in a text editor: the values are plain frontmatter keys.
5. **Rename a property** used by several entries: every entry's file is updated.
6. **Offline:** turn networking off and repeat 1–4. Nothing should change (Forma makes no network requests except loading `https:` images in notes).
7. **Rebuild:** Settings → Data → Rebuild search index; search still works.

**Phase 2** (still worth repeating)
1. **Persistence:** ⌘N, type a title, press Enter, type text, wait for "Saved", quit (⌘Q), reopen.
2. **Nesting and moves:** + on a page, drag, or ⋯ → Move to…; restart; the folders in Finder match.
3. **Outside edits:** edit an open page's file in TextEdit; type in Forma and change the file again: the conflict banner appears.
4. **Trash:** trash a page with children, then restore it from Trash.

## Design Decisions Confirmed (2026-11-02)

After the implementation audit, the user reviewed the decisions that the PRD hadn't spelled out or that differed from its illustrations:

| Decision | User's choice | Record |
|---|---|---|
| Page-link storage | `[Title](forma://page/<uuid>)`; `[[` is the typing trigger | ADR-007, confirmed |
| Workspace layout | The folder tree mirrors the page tree (`Name.md` / `Name/index.md`, attachments in `_assets/`) | ADR-004, ADR-008, confirmed |
| Collections | A collection is a page, with its schema in frontmatter and its entries as child pages | ADR-013, confirmed |
| Page title | Stored only in frontmatter `title`, not repeated as an H1 | ADR-006, confirmed |
| Search index location | Outside the workspace (Application Support) | ADR-003, confirmed |
| Remote images in notes | **Load automatically** (`https:` only). AI output and AI previews still don't load them | **ADR-031, supersedes ADR-025** |
| License | Apache-2.0 | ADR-026, accepted |
| CLAUDE.md transparency rules | Adopted, with this file as the journal | CLAUDE.md |

Not separately reviewed, and kept as documented: favorites and order in `.forma/tree.json` (ADR-014), TypeScript 6 with Biome (ADR-016), menu-accelerator shortcuts (ADR-030), Act mode requiring tool-calling models, and chat transcripts not being saved. The bundle identifier `app.forma.desktop` stays provisional.

## Known Issues and Open Decisions

1. **Native interactions are unverified by Claude.** The manual steps above cover them.
2. **Rebuilding the index, or a cold first open, briefly blocks other workspace requests** (about 0.6 s at 5,000 pages; ADR-035).
3. **CJK search results are substring matches without relevance ranking** (ADR-036).
4. **Recent pages in ⌘P last for the session only.**
5. **Index caches of workspaces that were moved or deleted stay in Application Support** until removed by hand. Each is about 1.7 times the size of its workspace's Markdown.
6. **A collection's sidebar list stops at 50 entries** (ADR-037). The rest are in the collection, search and quick open.
7. **Wide tables scroll sideways inside the page's reading column.**
8. **Recovery buffers** are cleared by a later successful save only when the same editor kept them (Phase 2).
9. **The title field doesn't follow a title changed outside Forma** while the page is open (Phase 2).
10. **The frontend bundle is 1.35 MB** (one chunk, loaded from disk). Code splitting can wait for Phase 7.
11. **Each development build re-asks macOS for folder access** (ad-hoc signing; Phase 7).
12. **Open audit items:** F6, F8–F14 (IMPLEMENTATION_AUDIT §8, §9.3).
13. **Bundle identifier, icon, Intel build:** unchanged from Phase 1 (provisional; Phase 7).

## Next Phase

**Phase 4: AI Provider Integration** (IMPLEMENTATION_PLAN §7): OpenAI, Anthropic, OpenAI-compatible endpoints, Ollama and LM Studio, with keys in the Keychain and streaming chat. It starts only after your approval.
