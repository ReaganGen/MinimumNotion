// The optional first page (IMPLEMENTATION_PLAN §10.1). It's created only
// when the user asks for it, through the same create and save as any page.
// The body is in the editor's canonical form, so editing it changes nothing
// else (gettingStarted.test.ts).

export const GETTING_STARTED_TITLE = "Getting started";

export const GETTING_STARTED_BODY = `Diaspar keeps each page as a Markdown file in this workspace's folder. You can open, copy or back up those files with any other app. This page is one of them; delete it whenever you like.

## Writing

- Type \`/\` at the start of a line for headings, lists, to-dos, quotes, code, tables and math.
- Markdown shortcuts work as you type: \`#\` for a heading, \`-\` for a list, \`[]\` for a to-do.
- Type \`[[\` to link to another page. Links keep working when pages are renamed or moved.
- Changes save on their own as you type.

## Organizing

- Drag pages in the sidebar to nest them. A page with pages inside is a folder on disk.
- Right-click a page and choose Turn into a collection to give its pages properties, then sort and filter them as a table.
- Deleted pages go to the Trash first, where you can restore them.

## Keyboard

| Action         | Keys |
| -------------- | ---- |
| New page       | ⌘N   |
| Search         | ⌘K   |
| Quick open     | ⌘P   |
| Toggle sidebar | ⌘⇧L  |
| Settings       | ⌘,   |

## AI, if you want it

AI is off until you turn it on in Settings → AI. It can use a model on this Mac (Ollama or LM Studio) or a cloud provider with your own API key, and Diaspar says whenever something is sent off your Mac. When it suggests changes to your pages, you review them first, and you can undo them afterwards.

## Keeping your notes safe

Settings → Data can export the pages as plain Markdown, or back up the whole workspace to a zip file that you can restore later.
`;
