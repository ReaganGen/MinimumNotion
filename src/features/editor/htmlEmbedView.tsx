// The live HTML embed (ADR-049): the HTML in a sandboxed frame, or its code.
// The frame has an empty `sandbox` (no scripts, forms, popups or navigation
// of the app, and an opaque origin), a policy that allows no requests
// (htmlEmbed.ts), and takes no clicks or focus. Only this view renders an
// embed; AI chat and AI change previews show the code.
import { NodeViewWrapper, type ReactNodeViewProps, ReactNodeViewRenderer } from "@tiptap/react";
import { useId, useState } from "react";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  embedDocument,
  embedHeight,
  HtmlEmbed,
  MAX_EMBED_HEIGHT,
  MIN_EMBED_HEIGHT,
  withEmbedHeight,
} from "./htmlEmbed";

function HtmlEmbedView({ node, editor, selected, updateAttributes }: ReactNodeViewProps) {
  const code = String(node.attrs.code ?? "");
  const info = String(node.attrs.info ?? "html embed");
  const height = embedHeight(info);
  const [mode, setMode] = useState<"preview" | "code">(code.trim() ? "preview" : "code");
  const [heightDraft, setHeightDraft] = useState<string | null>(null);
  const editable = editor.isEditable;
  const id = useId();

  function commitHeight() {
    if (heightDraft === null) return;
    const value = Number(heightDraft);
    setHeightDraft(null);
    if (heightDraft.trim() !== "" && Number.isFinite(value) && value !== height) {
      updateAttributes({ info: withEmbedHeight(info, value) });
    }
  }

  return (
    <NodeViewWrapper className="html-embed" data-selected={selected || undefined}>
      <div className="html-embed-bar" contentEditable={false}>
        <span className="html-embed-label">HTML</span>
        <ToggleGroup
          type="single"
          value={mode}
          onValueChange={(value) => value && setMode(value as "preview" | "code")}
          aria-label="Show"
          className="mr-auto"
        >
          <ToggleGroupItem value="preview" className="h-6 px-2 text-[12.5px]">
            Preview
          </ToggleGroupItem>
          <ToggleGroupItem value="code" className="h-6 px-2 text-[12.5px]">
            Code
          </ToggleGroupItem>
        </ToggleGroup>
        {editable && (
          <label className="html-embed-height" htmlFor={`${id}-height`}>
            Height
            <input
              id={`${id}-height`}
              type="number"
              min={MIN_EMBED_HEIGHT}
              max={MAX_EMBED_HEIGHT}
              step={20}
              value={heightDraft ?? String(height)}
              onChange={(e) => setHeightDraft(e.target.value)}
              onBlur={commitHeight}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  commitHeight();
                }
              }}
            />
          </label>
        )}
      </div>
      {mode === "preview" ? (
        <iframe
          className="html-embed-frame"
          title="HTML embed"
          sandbox=""
          referrerPolicy="no-referrer"
          srcDoc={embedDocument(code)}
          // Static: nothing in the frame can be clicked or reached with the
          // keyboard, so no link or form in it can navigate, whatever it targets.
          tabIndex={-1}
          style={{ height, pointerEvents: "none" }}
        />
      ) : (
        <textarea
          className="html-embed-code"
          aria-label="HTML code"
          spellCheck={false}
          readOnly={!editable}
          value={code}
          placeholder="<p>Static HTML. Scripts don't run, and nothing is loaded from the web.</p>"
          style={{ minHeight: Math.min(height, 320) }}
          onChange={(e) => updateAttributes({ code: e.target.value })}
        />
      )}
    </NodeViewWrapper>
  );
}

export const LiveHtmlEmbed = HtmlEmbed.extend({
  addNodeView() {
    return ReactNodeViewRenderer(HtmlEmbedView);
  },
});
