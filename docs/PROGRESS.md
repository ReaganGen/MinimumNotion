# Forma Development Progress

## Current Phase

Phase 2: Document System and Editor (approved 2026-11-02). **Implemented and verified (commit `52c5d1e`); waiting for the user's approval to proceed to Phase 3.** ADR-033 was approved on 2026-11-02.

## Status

Phase 2 is complete as far as automated and runtime checks can show. The manual checks in the built app (below) are for the user. ADR-033, which refines CLAUDE.md rule 7, was approved by the user, and rule 7 was updated to match.

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

Run on 2026-11-02 at the end of Phase 2 (`npm run check` exits 0):

| Command | Result |
|---|---|
| `npm run typecheck` (app and Vite config) | Pass |
| `npx biome check .` | Pass, 75 files, 0 diagnostics |
| `npx vitest run` | 16 files, **119 tests passed** |
| `cargo fmt --all --check` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |
| `cargo test --workspace` | **92 passed**: 89 in `forma-core` (including 25 workspace integration tests against real temporary folders) and 3 in the shell |
| `npm audit` | 0 vulnerabilities |
| `npm run tauri -- build --bundles app` | Pass. `Forma.app` is 8.16 MiB, arm64. The JS bundle is 1.26 MB (388 KB gzipped) |
| `codesign --verify --deep --strict Forma.app` | Pass (ad-hoc, hardened runtime). Not notarized |

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

1. **First run:** "Create a workspace in Documents" (macOS asks for Documents access; allow it). Or "Open a folder…".
2. **Persistence:** ⌘N, type a title, press Enter, type some text, wait for "Saved", quit (⌘Q), reopen: the page and text are there. Look at the `.md` file in Finder or a text editor: it reads as plain Markdown, with a short frontmatter.
3. **Nesting:** hover a page, click +, name the child. Drag it onto another page, or use ⋯ → Move to…. Quit and reopen: the hierarchy is kept, and the folders in Finder match.
4. **Markdown:** use headings, lists, a to-do list, a quote, a code block, a table and `$x^2$`. Reload (switch pages and back): nothing changed.
5. **Images:** paste or drag an image into a page. It's saved in the page's `_assets/` folder.
6. **Outside edits:** with a page open and no unsaved typing, edit its file in TextEdit and save: Forma shows the new text. Then type in Forma and, within a second, change the file again elsewhere: the conflict banner appears. Try "Keep both".
7. **Trash:** ⋯ → Move to Trash on a page with children, then Trash → Restore: the whole subtree comes back.

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

1. **Native interactions are unverified by Claude** (see Tests). The manual steps above cover them.
2. **Recovery buffers** are cleared by a later successful save only when the same editor kept them. One left from a page switch stays until you resolve it in the dialog on the next launch.
3. **The title field doesn't follow a title changed outside Forma** while the page is open; the sidebar and breadcrumbs do.
4. **The frontend bundle is 1.26 MB** (one chunk, loaded from disk). Code splitting can wait for Phase 7.
5. **Each development build re-asks macOS for folder access** (ad-hoc signing). Release builds with a stable signing identity won't (Phase 7).
6. **Index in memory (ADR-032):** a very large workspace is rescanned on every launch until Phase 3 adds the SQLite cache.
7. **Open audit items:** F6, F8–F14 (IMPLEMENTATION_AUDIT §8, §9.3).
8. **Bundle identifier, icon, Intel build:** unchanged from Phase 1 (provisional; Phase 7).

## Next Phase

**Phase 3: Collections, Links, and Search** (IMPLEMENTATION_PLAN §6): the SQLite index with FTS5, page links and backlinks, ⌘K search and ⌘P quick open, and collections. It starts only after your approval.
