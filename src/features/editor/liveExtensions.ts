// Extensions for the live editor: the Markdown schema from extensions.ts plus
// UI-only behavior (images and players, file cards, bookmarks, HTML embeds,
// live link titles, placeholder, "/" and "[[" menus, the highlight of a cited
// passage). Saving uses the same schema and Markdown rules the round-trip
// tests cover.
import type { AnyExtension } from "@tiptap/core";
import { Image } from "@tiptap/extension-image";
import { Placeholder } from "@tiptap/extensions";
import { assetUrl, ipc } from "@/lib/ipc/client";
import { LiveCallout } from "./calloutView";
import { LiveAttachment, LiveBookmark } from "./cardViews";
import { createMarkdownExtensions } from "./extensions";
import { LiveHtmlEmbed } from "./htmlEmbedView";
import { LivePageLink, LiveWikiLink } from "./linkViews";
import { mediaKind } from "./media";
import { PageLinkSuggestion } from "./pageLinkSuggestion";
import { Flash } from "./reveal";
import { SlashCommands } from "./slashCommands";

/**
 * Where an image's `src` loads from (ADR-031): references inside the
 * workspace go through `diaspar-asset://`, `https:` images load directly, and
 * anything else (plain `http:`, `data:`, `file:`) isn't loaded.
 */
export function resolveImageSource(pageId: string, src: string): string | null {
  const trimmed = src.trim();
  if (/^https:\/\//i.test(trimmed)) return trimmed;
  if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed) || trimmed.startsWith("//") || trimmed === "")
    return null;
  return assetUrl(pageId, trimmed);
}

/**
 * Where an audio or video `src` loads from: only references inside the
 * workspace. Remote media is shown as a link and never loaded (ADR-049; the
 * remote image exception of ADR-031 covers images only).
 */
export function resolveMediaSource(pageId: string, src: string): string | null {
  const trimmed = src.trim();
  if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed) || trimmed.startsWith("//") || trimmed === "")
    return null;
  return assetUrl(pageId, trimmed);
}

function renderPlayer(dom: HTMLElement, pageId: string, kind: "audio" | "video", attrs: Attrs) {
  const src = attrs.src ?? "";
  const source = resolveMediaSource(pageId, src);
  if (!source) {
    const note = document.createElement("span");
    note.className = "diaspar-media-link";
    note.textContent = `${kind === "audio" ? "Audio" : "Video"} not loaded: ${src}`;
    dom.append(note);
    if (/^https?:\/\//i.test(src.trim())) {
      const open = document.createElement("button");
      open.type = "button";
      open.textContent = "Open in browser";
      open.addEventListener("click", () => {
        ipc.openExternal(src.trim()).catch(() => undefined);
      });
      dom.append(open);
    }
    return;
  }
  const player = document.createElement(kind);
  player.controls = true;
  player.preload = "metadata";
  player.src = source;
  if (attrs.alt) player.setAttribute("aria-label", attrs.alt);
  if (attrs.title) player.title = attrs.title;
  player.addEventListener("error", () => dom.classList.add("is-broken"), { once: true });
  dom.append(player);
  if (attrs.alt) {
    const caption = document.createElement("span");
    caption.className = "diaspar-media-caption";
    caption.textContent = attrs.alt;
    dom.append(caption);
  }
}

type Attrs = Record<string, string | null>;

function imageView(pageId: string) {
  return Image.extend({
    addNodeView() {
      return ({ node }) => {
        const dom = document.createElement("span");
        const render = (attrs: Attrs) => {
          dom.replaceChildren();
          dom.classList.remove("is-broken");
          const kind = mediaKind(attrs.src ?? "");
          if (kind !== "image") {
            dom.className = `diaspar-media diaspar-${kind}`;
            renderPlayer(dom, pageId, kind, attrs);
            return;
          }
          dom.className = "diaspar-image";
          const source = resolveImageSource(pageId, attrs.src ?? "");
          if (source) {
            const img = document.createElement("img");
            img.src = source;
            img.alt = attrs.alt ?? "";
            if (attrs.title) img.title = attrs.title;
            img.draggable = false;
            img.addEventListener("error", () => dom.classList.add("is-broken"), { once: true });
            dom.append(img);
          } else {
            const placeholder = document.createElement("span");
            placeholder.className = "diaspar-image-placeholder";
            placeholder.textContent = `Image not loaded: ${attrs.src ?? ""}`;
            dom.append(placeholder);
          }
        };
        render(node.attrs);
        let current = node;
        return {
          dom,
          update: (updated) => {
            if (updated.type.name !== "image") return false;
            if (
              updated.attrs.src !== current.attrs.src ||
              updated.attrs.alt !== current.attrs.alt ||
              updated.attrs.title !== current.attrs.title
            ) {
              render(updated.attrs);
            }
            current = updated;
            return true;
          },
          // The player's controls and the Open button handle their own clicks.
          stopEvent: (event) =>
            event.target instanceof HTMLMediaElement || event.target instanceof HTMLButtonElement,
          ignoreMutation: () => true,
        };
      };
    },
  });
}

export function createEditorExtensions(pageId: string): AnyExtension[] {
  const live: Record<string, AnyExtension> = {
    image: imageView(pageId).configure({ inline: true }),
    callout: LiveCallout,
    attachment: LiveAttachment.configure({ pageId }),
    bookmark: LiveBookmark,
    htmlEmbed: LiveHtmlEmbed,
    pageLink: LivePageLink,
    wikiLink: LiveWikiLink,
  };
  return [
    ...createMarkdownExtensions().map((extension) => live[extension.name] ?? extension),
    Placeholder.configure({
      placeholder: ({ node }) =>
        node.type.name === "heading" ? "Heading" : "Write, or type “/” for commands…",
      showOnlyCurrent: true,
    }),
    SlashCommands,
    PageLinkSuggestion,
    Flash,
  ];
}
