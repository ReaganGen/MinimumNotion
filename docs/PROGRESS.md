# Forma Development Progress

## Current Phase

Phase 1: macOS Application Foundation

## Status

Implemented and verified (details below). An implementation audit ([IMPLEMENTATION_AUDIT.md](IMPLEMENTATION_AUDIT.md)) found four defects, F1–F4, which were fixed in a Phase 1.1 batch. **Awaiting user approval** to proceed to Phase 2.

## Completed Phases

- **Phase 0: Architecture and Project Planning.** Approved 2026-11-02 (commit `0b64019`).

## Implemented Features

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

Run on 2026-11-02 after the Phase 1.1 fixes:

| Command | Result |
|---|---|
| `npm run typecheck` (app and Vite config) | Pass |
| `npx biome check .` | Pass, 0 diagnostics |
| `npx vitest run` | 8 files, **45 tests passed** |
| `cargo fmt --all --check` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |
| `cargo test --workspace` | **30 passed**: 27 in `forma-core` (18 behavior tests + 9 generated binding exports) and 3 in the shell |
| `npm audit` | 0 vulnerabilities |
| `npx tauri build` | Pass. `Forma.app` is 5.2 MiB, arm64, `LSMinimumSystemVersion` 13.0 |
| `codesign --verify --deep --strict Forma.app` | Pass: "valid on disk", flags `adhoc,runtime`, `Info.plist` bound, resources sealed. Not notarized |

(Before Phase 1.1 this table said "ad-hoc signed", but the bundle actually failed strict verification. See audit finding F2.)

Manual and runtime checks:

- **Phase 1.1, re-signed app:** visible 0.53–0.80 s after launch over three runs.
- **Phase 1.1, F3:** with the settings folder temporarily set to mode `000`, the app opened in 0.52 s, kept running and quit cleanly. Permissions were restored afterwards, and the settings files were confirmed unchanged by hash. Whether the notice appears in the native window is **Unverified**: the session can't take screenshots.
- **Phase 1.1, F1:** the generated ACL (`src-tauri/gen/schemas/capabilities.json`) contains `core:window:allow-internal-toggle-maximize`. The double-click itself is **Unverified** at runtime, because the session can't send mouse input.

- **Release app launch, measured with a CoreGraphics window-list probe:** the window became visible 0.6–1.0 s after launch over three runs. That is well before the 3 s fallback, so the production JavaScript runs under the CSP and the `get_settings` and `app_ready` IPC calls are permitted.
  - The first build revealed a bug: `requestAnimationFrame` never fires in a hidden WKWebView window, so the window only appeared through the fallback, after 3.6 s. It's fixed by signalling from a post-commit effect.
- **Visual check in the browser preview:** light and dark themes, the AI panel docked, the Settings dialog with a working theme switch, ⌘K search, Esc closing dialogs, and overlay mode at 700 px wide.
- **Not verified by Claude:** the look and feel of the native window (traffic-light placement, menu bar, accelerators, and native appearance switching in the real app). This session has no Screen Recording or Accessibility permission, so it can't take screenshots of the app or send it keystrokes. See the manual steps below.

## Manual Verification Steps (for the user)

1. `open target/release/bundle/macos/Forma.app` (or run `npm run tauri:dev`).
2. Check that the traffic lights sit in the sidebar's top-left corner and the window has no visible title text.
3. Press ⌘⇧L and ⌘⇧A: the panels slide closed and open. Look for the matching items in the View menu.
4. Drag the sidebar's right edge to resize it, double-click the edge to reset it, then quit and relaunch: the width and visibility should be kept.
5. ⌘, → Appearance → Dark: the title bar and content switch together. Choose System, then change macOS appearance: Forma follows it live.
6. ⌘K and ⌘P open their dialogs, and Esc closes them.
7. Make the window narrow: the AI panel, then the sidebar, float over the editor instead of squeezing it.

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

1. **License:** resolved. Apache-2.0 was chosen and `LICENSE` added (ADR-026). `CONTRIBUTING.md` and a trademark policy are Phase 7 tasks.
2. **Bundle identifier** `app.forma.desktop` is used provisionally. It can be changed cheaply until Phase 4 stores Keychain items.
3. **The app icon is provisional**, a simple "F" mark. The final icon is a Phase 7 task.
4. **Build targets:** arm64 only so far. The universal (Intel) build and the DMG are Phase 7.
5. **Accelerator labels:** the labels in `SHORTCUTS` (TypeScript) mirror the accelerators in `menu.rs` by hand. A test checks the command IDs but not the key labels.
6. **The dev browser preview** can't show ⌘, inside the Claude desktop app's browser pane, because the host app captures that key. This doesn't affect the real app, where ⌘, is a native menu item.
7. **CLAUDE.md "Execution Transparency" section:** resolved. The user adopted it on 2026-11-02, with its journal pointed at this file instead of a separate `IMPLEMENTATION_STATUS.md`.
8. **Open audit items** (not part of Phase 1.1): F5 (atomic replace drops file metadata, which must be decided before Phase 2), F7 (no logging), F10 (constants duplicated across TypeScript and Rust), and the remaining F6, F8, F9 and F11–F14. The format decisions in audit §5.2 are now confirmed (see below). See [IMPLEMENTATION_AUDIT.md](IMPLEMENTATION_AUDIT.md) §8.

## Next Phase

**Phase 2: Document System and Editor.** It opens with the Markdown round-trip spike (ADR-015). Then: workspace initialization and opening, the Document Service (atomic writes, revisions, history, trash, recovery), the page tree, the Tiptap editor, autosave and external-change handling. See IMPLEMENTATION_PLAN §5.
