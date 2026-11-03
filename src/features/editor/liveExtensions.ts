// Extensions for the live editor: the Markdown schema from extensions.ts plus
// UI-only behavior (image loading, live link titles, placeholder, "/" and "[["
// menus). Saving uses the
// same schema and Markdown rules the round-trip tests cover.
import type { AnyExtension } from "@tiptap/core";
import { Image } from "@tiptap/extension-image";
import { Placeholder } from "@tiptap/extensions";
import { assetUrl } from "@/lib/ipc/client";
import { createMarkdownExtensions } from "./extensions";
import { LivePageLink, LiveWikiLink } from "./linkViews";
import { PageLinkSuggestion } from "./pageLinkSuggestion";
import { SlashCommands } from "./slashCommands";

/**
 * Where an image's `src` loads from (ADR-031): references inside the
 * workspace go through `forma-asset://`, `https:` images load directly, and
 * anything else (plain `http:`, `data:`, `file:`) isn't loaded.
 */
export function resolveImageSource(pageId: string, src: string): string | null {
  const trimmed = src.trim();
  if (/^https:\/\//i.test(trimmed)) return trimmed;
  if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed) || trimmed.startsWith("//") || trimmed === "")
    return null;
  return assetUrl(pageId, trimmed);
}

function imageView(pageId: string) {
  return Image.extend({
    addNodeView() {
      return ({ node }) => {
        const dom = document.createElement("span");
        dom.className = "forma-image";
        const render = (attrs: Record<string, string | null>) => {
          dom.replaceChildren();
          const source = resolveImageSource(pageId, attrs.src ?? "");
          if (source) {
            const img = document.createElement("img");
            img.src = source;
            img.alt = attrs.alt ?? "";
            if (attrs.title) img.title = attrs.title;
            img.draggable = false;
            img.addEventListener("error", () => dom.classList.add("is-broken"), { once: true });
            dom.classList.remove("is-broken");
            dom.append(img);
          } else {
            const placeholder = document.createElement("span");
            placeholder.className = "forma-image-placeholder";
            placeholder.textContent = `Image not loaded: ${attrs.src ?? ""}`;
            dom.append(placeholder);
          }
        };
        render(node.attrs);
        return {
          dom,
          update: (updated) => {
            if (updated.type.name !== "image") return false;
            render(updated.attrs);
            return true;
          },
        };
      };
    },
  });
}

export function createEditorExtensions(pageId: string): AnyExtension[] {
  const live: Record<string, AnyExtension> = {
    image: imageView(pageId).configure({ inline: true }),
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
  ];
}
