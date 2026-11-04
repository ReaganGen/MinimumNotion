# Diaspar User Guide

Diaspar 0.1.0 for macOS 13 or later, on Apple silicon and Intel Macs.

Diaspar keeps your notes as Markdown files in a folder you choose. Everything works offline. AI help is optional and off until you turn it on.

- [Installing](#installing)
- [Your workspace](#your-workspace)
- [Writing](#writing)
- [Organizing](#organizing)
- [Search](#search)
- [AI (optional)](#ai-optional)
- [Export, backup and restore](#export-backup-and-restore)
- [When something goes wrong](#when-something-goes-wrong)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Uninstalling](#uninstalling)

## Installing

1. Open `Diaspar_0.1.0_universal.dmg` and drag **Diaspar** into **Applications**.
2. Open Diaspar from Applications.

**The first time, macOS blocks it.** Diaspar isn't signed with an Apple Developer ID or notarized by Apple (the project has no Developer ID), so macOS says it can't verify that the app is free of malware. To open it anyway:

- **macOS 15 Sequoia and later:** open Diaspar once and close the message (click Done). Then open **System Settings → Privacy & Security**, scroll to **Security**, find "Diaspar was blocked to protect your Mac" and click **Open Anyway**. Confirm with your password or Touch ID, then click **Open**.
- **macOS 13 Ventura and 14 Sonoma:** in Finder, Control-click Diaspar in Applications, choose **Open**, then click **Open** in the dialog. The System Settings route above works too.

macOS remembers this for this copy of the app. After you install a new version, you'll be asked again.

If you'd rather check what you're running first: the release page lists the DMG's SHA-256 checksum. Compare it with `shasum -a 256 Diaspar_0.1.0_universal.dmg` in Terminal.

## Your workspace

A workspace is a folder. The first screen offers two choices:

- **Create a workspace in Documents** makes `~/Documents/Diaspar`.
- **Open a folder…** uses any folder you pick, including one that already has Markdown files.

Diaspar adds one hidden folder, `.diaspar`, for history, the trash and other bookkeeping. It doesn't change your files when you open them; only your edits are written. If you pick a very large folder (more than 10,000 entries, like all of Documents), Diaspar asks first.

macOS may ask whether Diaspar can access files in your Desktop or Documents folder. Diaspar needs that to open a workspace there. If you said no, allow it in **System Settings → Privacy & Security → Files and Folders**.

In an empty workspace, **Add a getting-started page** creates a short page about Diaspar. It's an ordinary page; delete it when you like. Nothing is created unless you click it.

**How your files are laid out:** a page is `Name.md`. A page with pages inside becomes a folder, `Name/index.md`, with its sub-pages next to it and its images in `Name/_assets/`. Each page's ID and properties are in the YAML frontmatter at the top of its file. You can edit the files with other apps while Diaspar is open; Diaspar picks up the change, and if you were editing the same page, it shows both versions instead of overwriting either.

## Writing

- Type `/` at the start of a line, or after a space, for the block menu. Under **Basic**:
  - text, headings, lists, to-dos, quotes, code blocks, dividers and tables;
  - **Page**, which makes a page inside this one, links to it and opens it;
  - **Callout**, a highlighted box for a note, tip or warning;
  - **Math block** and **Inline math** (LaTeX).

  Under **Media**, **Image**, **Audio**, **Video** and **File** let you choose a file, which is copied into the page's `_assets/` folder. Under **Embed** are **Bookmark** and **HTML**. Keep typing to filter the menu.
- **Callouts** are saved as `> [!NOTE]` quotes, the way GitHub and Obsidian write them. Click a callout's icon to change its type (Note, Tip, Important, Warning or Caution). Backspace at its start turns it back into plain text.
- Markdown shortcuts work as you type: `#` and a space for a heading, `-` for a list, `1.` for a numbered list, `[]` for a to-do, `>` for a quote.
- Type `[[` to link to another page. Links are stored as `[Title](diaspar://page/<id>)`, so they keep working when pages are renamed or moved.
- Paste or drop files to attach them to the page. Images, audio and video are shown in place, with a player for audio and video. Other files become a **file card** with its name, type and size: **Open** opens it in its usual app, and the folder button shows it in Finder. Diaspar never opens programs or scripts from a card; use Show in Finder for those.
- **Bookmarks** are a card for a web page: you type the address, a title and, if you like, a description. Diaspar never visits the page. Click the card to open it in your browser, or the pencil to edit it. It's saved as an ordinary link, `[Title](https://… "Description")`.
- **HTML embeds** show a piece of static HTML. Switch between **Preview** and **Code** in its bar, and set its height there. The HTML can't run scripts, load anything from the web, or be clicked; it's saved as an ```` ```html embed ```` code block, which other apps show as code.
- A card or player is alone in its paragraph. Press Return after it to keep writing; text typed right next to a file card or bookmark turns it back into a plain link the next time the page opens.
- ⌘-click a link to open it. Web links open in your browser.
- Changes save on their own. Leaving the page, or the app, saves at once.

Images from the web (`https:`) in your notes are loaded, so their servers can see when you open the page. Audio and video from the web are never loaded; they show as a link you can open in your browser. Images that AI writes or proposes are never loaded while you review them, and the AI panel shows HTML embeds as their code.

## Organizing

- **New page:** ⌘N, or **New page** in the sidebar.
- **Folders:** **New folder** in the sidebar, or **Add a folder inside** in a page's or folder's menu, makes a plain folder for grouping pages.
  - Its name is selected, so you can type a new one right away.
  - On disk it's an ordinary folder with nothing else inside it.
  - Clicking a folder opens and closes it. Opening it from search shows what's inside.
  - A folder has no text, icon or favorite. **Turn into page** gives it text, as a page with the same contents.
- **Nest pages:** drag them in the sidebar, or right-click → **Move to…**.
- **Favorites:** right-click a page → **Add to Favorites**.
- **Collections:** right-click a page → **Turn into a collection**. Its sub-pages become entries with properties (text, number, date, checkbox, select, multi-select), shown as a table or a list that you can sort and filter. Properties are saved in each entry's frontmatter.
- **Backlinks:** the bottom of a page lists the pages that link to it.
- **Trash:** **Move to Trash** keeps the page, with its sub-pages, in the Trash until you choose **Delete for good**. **Restore** puts it back.

## Search

- **Search** (⌘K) searches every page's title and text, including Chinese, Japanese and Korean text, and shows where each match is.
- **Quick open** (⌘P) jumps to a page by title, starting with recent pages.

The search index is kept outside the workspace and rebuilt when needed. If results look stale, **Settings → Data → Rebuild search index**.

## AI (optional)

AI is off until you enable it in **Settings → AI**. You can use:

- **A model on this Mac:** Ollama or LM Studio. Nothing leaves your Mac.
- **A cloud provider:** OpenAI, Anthropic, or any OpenAI-compatible server, with your own API key. A ChatGPT or Claude subscription doesn't include API access; you need an API key from the provider.

API keys are stored in the macOS Keychain, never in a file. Before anything goes to a cloud provider for the first time, Diaspar asks you to agree, and each message shows where it was sent ("Stays on this Mac" or the provider's name).

Open the AI panel with ⌘⇧A. It has two modes:

- **Ask** answers from your notes and cites the passages it used, as numbered sources you can click. It changes nothing.
- **Edit** proposes changes: new pages, edits, moves, collections, property values and links. Nothing changes until you click **Review changes**, look at each change word by word, and **Apply** the ones you want. All chosen changes are applied together or not at all. If a page changed since the proposal, Diaspar tells you instead of overwriting it. Edit needs a model that can call tools.

**AI changes** (the history button in the panel) lists what you applied. **Undo** reverts a change set while its pages are as it left them; undoing an undo redoes it.

Chats aren't saved; they're gone when you quit.

## Export, backup and restore

In **Settings → Data**:

- **Export…** copies your pages and images, without `.diaspar`, into a new folder named "*workspace* export *date*". Use it to take your notes to another app.
- **Back up…** saves the whole workspace, `.diaspar` included (history, trash and AI change records), as one zip file.
- **Restore…** asks for a backup zip and a place to put it, unpacks it into a **new** folder there, and opens it. It never overwrites an existing folder. It refuses archives with entries that would land outside that folder.

A backup is an ordinary zip, so you can also unpack it yourself.

Your workspace is a normal folder, so Time Machine and other backup tools work too. Sync services like iCloud Drive or Dropbox aren't officially supported in 0.1.0: they can create conflict copies while a file is being written.

## When something goes wrong

- **A page shows a notice about its frontmatter:** the YAML at the top of the file can't be read. Diaspar shows the page read-only until you fix it in a text editor.
- **The workspace opened read-only:** the notice says why (for example, it was created by a newer version of Diaspar, its `.diaspar/workspace.json` can't be read, or it's open in another Diaspar window).
- **Your settings were reset:** if the settings file couldn't be read, Diaspar starts with defaults and keeps the old file next to it as `settings.json.corrupt-<time>`.
- **Unsaved text after a crash:** the next launch offers it back.

**Reporting a problem:** **Settings → Data → Diagnostics**. **Show log** shows Diaspar's log on this Mac. **Copy diagnostics** copies a report for a bug report: the Diaspar and macOS versions, a summary of your settings (theme, AI on or off, provider types, whether a key is stored), and recent log lines with every path and quoted name replaced. It contains no note text, page titles, API keys or server addresses. Paste it and read it before you send it.

The full log is `~/Library/Logs/app.diaspar.desktop/diaspar.log`. It never contains note text or keys, but it does contain file and folder names.

## Keyboard shortcuts

| Action | Keys |
| --- | --- |
| New page | ⌘N |
| Search | ⌘K |
| Quick open | ⌘P |
| Toggle sidebar | ⌘⇧L |
| Toggle AI panel | ⌘⇧A |
| Settings | ⌘, |
| Close a dialog or floating panel | Esc |

Standard macOS editing shortcuts (⌘Z, ⌘C, ⌘V, ⌘B, ⌘I…) work as usual.

## Uninstalling

1. Quit Diaspar and move it from Applications to the Trash.
2. Optionally remove its app data: `~/Library/Application Support/app.diaspar.desktop` (settings and search index) and `~/Library/Logs/app.diaspar.desktop`.
3. If you used a cloud AI provider, remove the keys in **Keychain Access**: search for `app.diaspar.desktop.ai-provider`. (Removing a provider in Settings → AI also deletes its key.)

Your workspace folders are yours; uninstalling doesn't touch them. The hidden `.diaspar` folder inside each can be deleted if you no longer use Diaspar.
