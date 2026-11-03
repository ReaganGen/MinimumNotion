// Live views of page links and wikilinks: current titles, and honest states
// for links whose target is in the trash, missing, or ambiguous. Text only;
// nothing from a page is ever inserted as HTML.
import type { Node as PMNode } from "@tiptap/pm/model";
import {
  currentTitle,
  requestPage,
  requestWiki,
  useLinkStore,
  wikiTarget,
} from "@/features/links/linkStore";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { PageLink, WikiLink } from "./pageLink";

function linkView(node: PMNode, paint: (dom: HTMLElement, node: PMNode) => void, type: string) {
  const dom = document.createElement("span");
  dom.contentEditable = "false";
  let current = node;
  const render = () => paint(dom, current);
  render();
  // A click follows the link (when it goes somewhere): links are atoms, so
  // there's no text inside them to place a cursor in.
  dom.addEventListener("click", (event) => {
    const destination = linkDestination(current);
    if (!destination) return;
    event.preventDefault();
    useWorkspaceStore.getState().openPage(destination);
  });
  const unsubscribe = useLinkStore.subscribe(render);
  return {
    dom,
    update: (updated: PMNode) => {
      if (updated.type.name !== type) return false;
      current = updated;
      render();
      return true;
    },
    destroy: unsubscribe,
  };
}

function paintPageLink(dom: HTMLElement, node: PMNode) {
  const id = node.attrs.id as string;
  requestPage(id);
  const info = useLinkStore.getState().pages[id];
  const label = (node.attrs.label as string) || "Untitled";
  dom.className = "page-link";
  dom.dataset.state = info?.state ?? "loading";
  dom.textContent =
    info?.state === "ok" ? `${info.icon ? `${info.icon} ` : ""}${info.title}` : label;
  dom.title =
    info?.state === "trashed"
      ? "This page is in the Trash. Restoring it brings the link back."
      : info?.state === "missing"
        ? "No page has this link's ID."
        : "";
}

function paintWikiLink(dom: HTMLElement, node: PMNode) {
  const source = node.attrs.source as string;
  const target = wikiTarget(source);
  requestWiki(target);
  const resolution = useLinkStore.getState().wikis[target];
  const alias = source
    .replace(/^\[\[|\]\]$/g, "")
    .split("|")[1]
    ?.trim();
  dom.className = "wiki-link";
  dom.dataset.state = !resolution ? "loading" : resolution.target ? "ok" : "unresolved";
  dom.textContent = resolution?.target ? (alias ?? resolution.target.title) : source;
  dom.title = !resolution
    ? ""
    : resolution.target
      ? `${source}`
      : resolution.matches > 1
        ? `${resolution.matches} pages match “${target}”. Rename one, or use a page link.`
        : `No page called “${target}”.`;
}

export const LivePageLink = PageLink.extend({
  addNodeView() {
    return ({ node }) => linkView(node, paintPageLink, "pageLink");
  },
}).configure({ currentTitle });

export const LiveWikiLink = WikiLink.extend({
  addNodeView() {
    return ({ node }) => linkView(node, paintWikiLink, "wikiLink");
  },
});

/** Where a click on a link node goes, if anywhere. */
export function linkDestination(node: PMNode): string | null {
  if (node.type.name === "pageLink") {
    const info = useLinkStore.getState().pages[node.attrs.id as string];
    return info?.state === "ok" ? info.id : null;
  }
  if (node.type.name === "wikiLink") {
    const target = wikiTarget(node.attrs.source as string);
    return useLinkStore.getState().wikis[target]?.target?.id ?? null;
  }
  return null;
}
