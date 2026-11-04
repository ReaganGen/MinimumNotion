// The live callout (ADR-048): an icon that opens the type menu, then the
// callout's blocks. Only the live editor uses this view; saving goes through
// callout.ts.
import {
  NodeViewContent,
  NodeViewWrapper,
  type ReactNodeViewProps,
  ReactNodeViewRenderer,
} from "@tiptap/react";
import {
  AlertTriangle,
  Info,
  Lightbulb,
  type LucideIcon,
  MessageSquareWarning,
  OctagonAlert,
  StickyNote,
} from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { CALLOUT_KINDS, Callout, calloutKind } from "./callout";

const ICONS: Record<string, LucideIcon> = {
  note: Info,
  tip: Lightbulb,
  important: MessageSquareWarning,
  warning: AlertTriangle,
  caution: OctagonAlert,
  other: StickyNote,
};

function CalloutView({ node, editor, updateAttributes }: ReactNodeViewProps) {
  const kind = calloutKind(node.attrs.marker);
  const Icon = ICONS[kind.id] ?? StickyNote;
  const title = String(node.attrs.title ?? "").trim();
  return (
    <NodeViewWrapper className="callout" data-tone={kind.tone}>
      <div className="callout-icon" contentEditable={false}>
        <DropdownMenu>
          <DropdownMenuTrigger asChild disabled={!editor.isEditable}>
            <button type="button" aria-label={`${kind.label} callout. Change type`}>
              <Icon className="size-4" aria-hidden />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-40">
            {CALLOUT_KINDS.map((option) => {
              const OptionIcon = ICONS[option.id] ?? StickyNote;
              return (
                <DropdownMenuItem
                  key={option.id}
                  onSelect={() => updateAttributes({ marker: option.id.toUpperCase() })}
                >
                  <OptionIcon aria-hidden /> {option.label}
                </DropdownMenuItem>
              );
            })}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <div className="callout-body">
        {title && (
          <div className="callout-title" contentEditable={false}>
            {title}
          </div>
        )}
        <NodeViewContent className="callout-content" />
      </div>
    </NodeViewWrapper>
  );
}

export const LiveCallout = Callout.extend({
  addNodeView() {
    return ReactNodeViewRenderer(CalloutView);
  },
});
