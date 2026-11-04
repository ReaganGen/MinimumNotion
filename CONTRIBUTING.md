# Contributing to Diaspar

Thanks for helping. Diaspar is small on purpose: a reliable, local-first notes app with optional AI. A smaller change that works completely is worth more than a larger one that doesn't.

## Setup

You need macOS 13 or later, the Xcode Command Line Tools, [rustup](https://rustup.rs) and Node 24 or later.

```bash
npm install
npm run tauri:dev     # the app, with hot reload
```

`npm run dev` alone serves the UI in a browser with an in-memory stand-in for the Rust side (`src/lib/ipc/browserPreview.ts`). It's handy for layout work, but nothing is saved and its search and AI are rough fakes.

CI uses Rust 1.99.1 (see `.github/workflows/ci.yml`); the minimum supported version is 1.90.

## Checks

```bash
npm run check         # tsc, Biome, Vitest, rustfmt, clippy -D warnings, cargo test
npm run format        # Biome formatting and import order
npm run bindings      # regenerate src/lib/ipc/bindings after changing a ts-rs type
```

Commit the regenerated bindings with the Rust change; CI fails if they differ. CI also runs `npm audit` and `cargo audit` (install it with `cargo install cargo-audit --locked`), and builds the universal app and DMG.

Performance on a large workspace:

```bash
node scripts/generate-workspace.mjs sandbox-workspaces/perf 5000
cargo run --release -p diaspar-core --example measure -- sandbox-workspaces/perf
```

`sandbox-workspaces/` is ignored by Git. Never point tools or tests at real notes.

## How the code is organized

- `crates/diaspar-core`: the workspace on disk, the Document Service, Markdown and frontmatter handling, the SQLite index, export and backup. No Tauri and no network.
- `crates/diaspar-ai`: providers, retrieval, the assistant's tools and change sets.
- `src-tauri`: the thin shell. Commands, menus, windows, capabilities.
- `src`: the React app.

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first. [docs/DATA_MODEL.md](docs/DATA_MODEL.md) is normative: code that disagrees with it is a bug unless the change updates it (and records why in [docs/DECISIONS.md](docs/DECISIONS.md)).

The rules that matter most:

1. Every change to a workspace file goes through the Document Service. The UI and the AI never write files.
2. IPC takes page IDs, not paths. Folders and files the user picks come from dialogs Rust opens.
3. Opening a page never writes it. Only real edits are saved, against the revision they started from; a conflict is shown, never overwritten.
4. Frontmatter is edited in place: unknown keys, comments and order survive.
5. AI only proposes changes. Applying them is a user action, and they can be undone.
6. API keys live in the Keychain only and never reach the webview, logs or the workspace.
7. The webview has no network access except `https:` images in notes. No raw HTML from documents or AI output.
8. No telemetry, accounts or cloud services.

## Changes

- **Tests.** New behavior comes with tests. Markdown dialect changes need a fixture in `tests/fixtures/markdown/` that survives the round trip.
- **Dependencies.** Add one only when it clearly pays for itself. Check its current release, that it's maintained and that it's OSI-licensed, pin the exact version, and record it in [docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md) §2.
- **UI.** Use the design tokens in `src/styles/tokens.css` (text contrast is tested in both themes). Every control must work; if something isn't built, don't show it. Keep standard macOS shortcuts.
- **Commits.** Descriptive messages in the form `feat: …`, `fix: …`, `docs: …`. Never commit keys, local settings or note content.

By contributing, you agree that your contribution is licensed under the [Apache License 2.0](LICENSE).

## Releases

1. Set the same version in `package.json`, `Cargo.toml` (`[workspace.package]`) and `src-tauri/tauri.conf.json`, update `docs/PROGRESS.md`, and commit.
2. Tag the commit `v<version>` and push the tag. `.github/workflows/release.yml` runs the checks, builds the universal app and DMG, verifies them (`scripts/verify-bundle.sh`) and creates a **draft** GitHub release with the DMG, its SHA-256 and the signing status.
3. Read the draft, try the DMG, and publish it by hand.

**Signing.** Releases are ad-hoc signed (`bundle.macOS.signingIdentity: "-"`) and not notarized, because the project has no Apple Developer ID. Users allow the app once in System Settings ([user guide](docs/USER_GUIDE.md#installing)). Every build has a new ad-hoc identity, so macOS also asks again for Keychain and folder access after an update.

Developer ID signing and notarization aren't set up. Tauri reads them from environment variables: `APPLE_CERTIFICATE` (a base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD` and `APPLE_SIGNING_IDENTITY` for signing, and `APPLE_API_ISSUER`, `APPLE_API_KEY` and `APPLE_API_KEY_PATH` (or `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`) for notarization. Adding them as repository secrets and passing them to the build step is the starting point; it hasn't been tested here.
