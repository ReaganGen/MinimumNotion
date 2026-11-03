# Forma Development Progress

## Current Phase

Phase 5: AI Knowledge Retrieval (approved 2026-11-03). **Implemented and verified; waiting for the user's approval to proceed to Phase 6.**

## Status

Phase 5 is complete as far as automated, preview and release-app checks can show. As in Phase 4, no API key, Ollama or LM Studio was available, so no real model has answered a question about the notes yet; that's the first manual check below.

### Phase 5 journal

- **Passages (ADR-041).** Each page body is cut at headings and between blocks into passages of 250–400 tokens, keeping their heading path and byte range (`forma-core/src/passages.rs`). Index schema 2 stores them with a stemmed word index, and an older index is rebuilt by itself (checked in the release app).
- **Retrieval.** `Retriever` with one BM25 implementation: any of the question's words (stemmed, without stop words), plus, for Chinese, Japanese and Korean, pages found through the trigram table whose passages are ranked by shared three-character pieces; the two rankings are merged by reciprocal rank, at most three passages per page. A labeled set of 17 questions (English and Chinese, one with no answer) finds the right page first for all 16 answerable ones and nothing for the other. The set was written for this test, so it guards against regressions rather than measuring quality.
- **Read-only by type (ADR-042).** The assistant reaches notes only through `ReadView`, a view of the workspace with no method that writes. The four Ask tools (`search_pages`, `read_page`, `list_pages`, `get_page_metadata`) each take that view and their arguments: page IDs only, typed schemas, results cut to fit.
- **Packing.** Each request is fitted to the model's window: the message, then the selection, the attached page (cut around the cursor if long), retrieved passages, then as many earlier turns as fit. Rust reports what went in, and the panel shows it under the message ("Included “Plan” and 3 passages from your notes.").
- **Sources and citations.** Every piece of the notes sent is a numbered source (`S1`, `S2`…, unique in the conversation). Citations that match a source the answer was given open the page and highlight the passage; any other key is marked unverified. Each answer lists the sources it cited, the ones it was given but didn't cite, and unverified keys.
- **Tools for capable models.** Up to 6 rounds of lookups, shown in the panel as they happen ("Searched the notes for “ice”"), then one round that must answer. Anthropic's thinking blocks go back with tool results, as its API requires. Settings → AI → "Look things up in your notes" (Automatic, On, Off) decides for models whose provider doesn't say (OpenAI).
- **Actions.** "Ask AI about this page" and "Summarize with AI" in the page menu; "Summarize this page" and "Ask about this page" in an empty conversation; "Explain" and "Ask AI" in the selection menu. They appear only when AI is on.
- **Privacy.** "Search notes" in the panel is on by default; off, nothing from the notes is sent unless attached, and no tools are offered. The cloud agreement now says that note excerpts go to the provider; agreements given under Phase 4's wording ("never your notes") no longer count, so a cloud provider asks once more.
- **Bugs found and fixed during the phase:**
  1. *A passage that didn't fit the budget still got a source key*, so the panel could list it as provided and accept a citation of it. Keys are now given only to text that's sent; the fuzz test checks that every key it sees was in the tool's output.
  2. *With "Search notes" off, tool-capable models could still read notes through tools*, so "no notes included" wasn't true. Tools now follow the switch.
  3. *Cold open slowed from 0.63 s to 2.2 s* on 5,000 pages. Deleting a page's passages used `rowid IN (subquery)`, which FTS5 answers by scanning the table (once per page), and a second trigram table cost 0.75 s. Both fixed: 0.83 s.
  4. *The last tool round sent no tool definitions* although the conversation used tools, which some APIs reject. It now keeps them, with `tool_choice: none`.
  5. *A page source's excerpt kept its Markdown*, so its citation couldn't find the text in the editor. Excerpts are plain text.
  6. *A very large source number from the window could overflow* (a panic in debug builds). It's clamped.
- **Design decisions:** ADR-041, ADR-042.

### Phase 4 journal

- **API check first (ADR-038, risk R8).** The providers' current documentation was read on 2026-11-03. OpenAI: Chat Completions still works, but its newest models can't call tools through it, so **the OpenAI preset uses the Responses API** (the user's choice), with `store: false`. Anthropic: API version `2026-10-21`, which removed `temperature` for every model. Ollama: native API, because only it takes `num_ctx`. LM Studio: Chat Completions, plus its own model list for details.
- **`forma-ai` crate.** The only crate with network access. Stream framing (server-sent events and JSON lines, safe at any chunk boundary), four protocol modules with pure decoders (text, tool-call and usage events, stop reasons, errors), and one `Provider` type over them (ADR-039). Model lists come from the providers; model details (context window, output limit, tool support) where the provider reports them.
- **Rules in Rust.** `AiService` refuses to send unless AI is on, a provider and model are chosen, a required key is stored, and, for a provider that isn't on this Mac, the user has agreed. It also refuses a conversation too big for the model's window rather than letting the provider cut it. Ollama always gets a `num_ctx` (the model's own window, capped at 8,192 tokens unless the user sets one).
- **Keys.** In the login keychain under `app.forma.desktop.ai-provider` (ADR-020). Typed once into a password field, sent to Rust, and never returned by any command. Read once per session. Any copy of a key in a provider's error message is replaced before it leaves Rust; AI failures are logged by kind and status only.
- **Settings → AI.** Turn AI on (off by default), add providers (OpenAI, Anthropic, Ollama, LM Studio, OpenAI-compatible), name and address, key (stored in the Keychain; Replace and Remove key), model (loaded from the provider or typed), what the provider says about the model, temperature (not for Anthropic), maximum output, context window, Test connection, and Remove provider with confirmation. A plain-`http` address to another machine is flagged as unencrypted.
- **AI panel.** Streaming chat with Stop, Retry and Clear. Each message says where it went and to which model ("Stays on this Mac" or "Sent to Anthropic (api.anthropic.com)"); the first message to a cloud provider needs explicit agreement. Answers are Markdown built as React elements (ADR-040): no HTML, no remote images, links open only on click and show their destination. The assistant sees no notes; that's Phase 5. Transcripts are kept for the session only.
- **Bugs found and fixed during the phase:**
  1. *A stream cut off mid-line was reported as a malformed response.* Unfinished frames at the end of a stream are now dropped, and the answer is reported as interrupted.
  2. *Two "Remove" buttons in one provider's settings* (key and provider). The key's is now "Remove key".
  3. *Task-list checkboxes in answers didn't render.*
  4. *The line saying where a message goes was cut off* in a narrow panel. It wraps now.
  5. *A CSS rule order warning left from Phase 3* (`[data-compact] .forma-editor`). The Phase 3 record said Biome reported 0 diagnostics; it reported this one warning.
- **Design decisions:** ADR-038, ADR-039, ADR-040. ADR-019's OpenAI preset changed (ADR-038). The decision index in DECISIONS.md now lists ADR-032 to ADR-037, which were missing.

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
- **Phase 3: Collections, Links, and Search.** Approved 2026-11-03 (commit `4c4ade6`).
- **Phase 4: AI Provider Integration.** Approved 2026-11-03 (commit `5e3825e`).

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

Run on 2026-11-03 at the end of Phase 5 (`npm run check` exits 0):

| Command | Result |
|---|---|
| `npm run typecheck` (app and Vite config) | Pass |
| `npx biome check .` | Pass, 0 diagnostics (115 files) |
| `npx vitest run` | 28 files, **184 tests passed** |
| `cargo fmt --all --check` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |
| `cargo test --workspace` | **246 passed, 2 ignored**: 159 in `forma-core`, 84 in `forma-ai` (65 unit, 18 against a local HTTP server, 1 key-leak check) and 3 in the shell |
| `npm audit` | 0 vulnerabilities (no dependency was added) |
| `npm run tauri -- build --bundles app` | Pass. `Forma.app` is 12.16 MiB, arm64 (11.97 MiB in Phase 4). The JS bundle is 1.39 MB (426 KB gzipped) |
| `codesign --verify --deep --strict Forma.app` | Pass (ad-hoc, hardened runtime). Not notarized |

The two ignored tests (a real Keychain item; the cloud APIs with a fake key) were run in Phase 4 and not again: the code they cover didn't change.

New in Phase 5: passage cutting (headings, size limits, long blocks cut at lines, sentences and characters, byte ranges); the passage tables (stemmed words, replaced and removed with their page, kept in step on rebuild); retrieval (question terms, quoting, the per-page cap, excluded pages, the labeled set); packing (sources numbered from where the conversation is, attached page and selection first, a long page cut around the cursor, older turns dropped first, the budget held, a message too long refused, source blocks that can't be closed from inside, a passage that doesn't fit gets no key); the four tools (results, parts of long pages, metadata, 13 kinds of bad arguments, and **a fuzz test** of 3,000 random calls with paths, `..`, SQL, FTS syntax, wrong types and broken JSON that checks no call panics, every key given was sent, and **no file in the workspace changes**); a type test pinning the Ask registry to read-only tools; each protocol's tool turns (Anthropic's blocks and thinking, Responses items, Chat Completions `tool` messages, Ollama `tool_name`), and, against a local HTTP server, an answer with numbered sources, a full tool round trip with Anthropic (thinking sent back, results in one user message), the round cap, the user's tool setting winning over the provider, and "Search notes" off sending no notes and no tools. In the UI: citations (formats, verification, an invented key), the reveal (finding text across blocks, the highlight changing nothing), the panel (attaching the page, turning search off, what was included, sources listed and opened, unverified keys, numbering across messages, Summarize), and AI items in the page menu only when AI is on. Settings: a Phase 4 agreement isn't carried over.

**Performance** with 5,000 generated pages (`cargo run --release -p forma-core --example measure -- sandbox-workspaces/perf`; Phase 4's code measured the same day on the same machine for comparison):

| Operation | Phase 4 | Phase 5 |
|---|---|---|
| Open, no index cache (cold) | 633 ms | 831 ms |
| Open, with the cache | 27 ms | 28 ms |
| Rebuild the index | 564 ms | 684 ms |
| Index size on disk | 35.2 MB | 51.3 MB |
| Retrieve 8 passages: English questions | — | 1.5–2.7 ms |
| Retrieve 8 passages: a Chinese question, `知识` | — | 2.9 ms, 7.7 ms |

Runtime check on the release app (2026-11-03), with AI off and a generated 40-page workspace: the app **opened no network sockets**, built a schema 2 index (41 pages, 59 passages), quit in 0.14 s, and on a second launch rebuilt an index whose version was set back to 1. Every file in the workspace was unchanged. The settings file and window state were restored byte-for-byte afterwards, and the test index and workspace deleted.

Checked in the development browser preview (fake provider and retrieval, no network), light and dark: turning AI on and choosing a local model, the empty conversation's page actions, "Add" and "Search notes", a question showing what was included, a cited source and an invented one (struck through, "Unverified: S99…"), the source list, a citation opening another page and highlighting the passage, the selection menu's Explain and Ask AI, and the selection chip. A fresh load and those steps logged no errors; duplicate-key warnings appeared only after hot reloads, as in Phase 4.

**Unverified:** any real model answering from the notes, calling the tools or citing (no keys, no Ollama or LM Studio here); every interaction inside the native window.

### Phase 4 test record

Phase 4 ran `npm run check` (173 frontend and 207 Rust tests then), a real Keychain round trip, the cloud APIs rejecting a fake key with 401 over TLS, and the release app with AI off (no sockets, quit in 0.16 s, files unchanged). New then: stream framing at every chunk size; each protocol's decoder on its fixtures; request bodies; HTTP status mapping; base-URL rules; against a local HTTP server, headers and bodies, streaming, Stop, error mapping, the connection test and every refusal before sending; the key-leak test; and the panel, the Markdown renderer and Settings → AI. **Stream fixtures are not recordings**: `crates/forma-ai/fixtures/` follows each provider's documented format.

### Phase 3 test record

Phase 3 ran `npm run check` (155 frontend and 143 Rust tests then). New in Phase 3: index (word, prefix, substring and CJK search; rebuild equivalence; cache across restarts; outside edits while closed), links (renames and moves, wikilink resolution and ambiguity, trash and restore, parity fixtures on both sides), collections (schema validation, the coercion table, value writing, renames and their refusal, crash rollback, turning back into a page, views, newer schemas read-only), and in the UI: search keyboard handling and stale answers, `[[` link insertion (and no link when the ID can't be saved), page link titles and states, backlinks, table editing, option creation, filters, list view, read-only collections, the rebuild button.

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

**Phase 5**
1. **Ask about notes:** set up a provider (Phase 4, steps 2–3), open the AI panel (⌘⇧A) and ask something a few of your test pages answer. Under your message it says what was included; the answer cites [S1], [S2]…; each citation and each entry under "Sources" opens its page with the passage highlighted.
2. **Chinese:** ask in Chinese about Chinese pages (for example a question containing a word from a page). The right passages should be cited.
3. **Missing information:** ask about something no page mentions. It should say "No notes matched." and the answer should say the notes don't cover it.
4. **Page and selection:** in the page menu, "Summarize with AI"; in a page, select text, then "Explain" or "Ask AI". The page or selection shows above the message box and in "Included…".
5. **Search notes off:** turn off "Search notes" in the panel and remove attachments: the line under the box says "no notes included", and the answer has no sources.
6. **Tools:** with a model that calls tools (Anthropic, or Settings → AI → "Look things up in your notes" → On for OpenAI), ask something that needs several pages. Lookups appear above the answer ("Searched the notes for…", "Read “…”").
7. **Cloud agreement:** a cloud provider agreed to in Phase 4 asks again, with the new wording.
8. **Nothing written:** after all of the above, no page file changed (Forma never writes from the assistant).

**Phase 4**
1. **AI off:** with AI off (the default), the panel (⌘⇧A) says so and Forma works as before.
2. **Local model:** install Ollama (or LM Studio) and pull a model. Settings → AI → Use AI → Add provider → Ollama. Load models, choose one, Test connection. Chat in the panel: the answer streams, the message says "Stays on this Mac", and Stop works mid-answer.
3. **Cloud provider:** add OpenAI or Anthropic, paste an API key, Save key. It shows "Stored in your Keychain." (macOS may ask to allow Forma). In Keychain Access, the item is under `app.forma.desktop.ai-provider`. Load models, choose one, Test connection. The first message asks you to agree; then the answer streams.
4. **Failures:** a wrong key (Test connection says the key wasn't accepted), a model ID that doesn't exist, Ollama stopped (the address can't be reached), and networking off with a cloud provider. Each message should say what went wrong.
5. **Restart:** quit and reopen. Providers, the chosen model and stored keys are kept; the conversation isn't.
6. **Remove:** Remove provider. The Keychain item is gone.

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
2. **No real AI conversation has been tested**, with or without notes. Stream fixtures and tool message formats follow the providers' documentation (ADR-038, ADR-042).
3. **Retrieval matches words and characters, not meaning** (ADR-041). A question phrased with none of the notes' words finds nothing; the assistant then says the notes don't cover it.
4. **Citations depend on the model.** One that ignores the instructions answers without citations; the panel still lists the sources it was given.
5. **OpenAI models report no context window or tool support.** Forma packs for 8,192 tokens and offers no tools unless the user sets a window or turns tools on in Settings → AI.
6. **A long attached page is cut to fit**, around the cursor if the page is open. The line under the message says "part of “…”".
7. **Development builds may ask for Keychain access again** after each rebuild (ad-hoc signing, risk R7).
8. **Rebuilding the index, or a cold first open, briefly blocks other workspace requests** (about 0.8 s at 5,000 pages; ADR-035, ADR-041). The index is 46% larger than in Phase 4.
9. **CJK search results are substring matches without relevance ranking** in ⌘K (ADR-036); the assistant's retrieval ranks them.
10. **Recent pages in ⌘P last for the session only.**
11. **Index caches of workspaces that were moved or deleted stay in Application Support** until removed by hand.
12. **A collection's sidebar list stops at 50 entries** (ADR-037).
13. **Wide tables scroll sideways inside the page's reading column.**
14. **Recovery buffers** are cleared by a later successful save only when the same editor kept them (Phase 2).
15. **The title field doesn't follow a title changed outside Forma** while the page is open (Phase 2).
16. **The frontend bundle is 1.39 MB** (one chunk, loaded from disk). Code splitting can wait for Phase 7.
17. **Each development build re-asks macOS for folder access** (ad-hoc signing; Phase 7).
18. **Open audit items:** F6, F8–F14 (IMPLEMENTATION_AUDIT §8, §9.3).
19. **Bundle identifier, icon, Intel build:** unchanged from Phase 1 (provisional; Phase 7).

## Next Phase

**Phase 6: AI Agent and Knowledge Management** (IMPLEMENTATION_PLAN §9): proposed changes as ChangeSets with a diff preview, approve or reject, conflict checks against each page's revision, undo from the operation log, and prompt-injection tests. It starts only after your approval.
