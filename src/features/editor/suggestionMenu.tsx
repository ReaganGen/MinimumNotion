// The popup list shared by the "/" block menu and "[[" page links: arrow keys
// move, Return chooses, Escape closes (handled by the suggestion plugin).
import { ReactRenderer } from "@tiptap/react";
import type { SuggestionKeyDownProps, SuggestionProps } from "@tiptap/suggestion";
import { forwardRef, type ReactNode, useImperativeHandle, useState } from "react";
import { cn } from "@/lib/utils";

export interface MenuHandle {
  onKeyDown: (props: SuggestionKeyDownProps) => boolean;
}

interface MenuListProps<Item> extends SuggestionProps<Item> {
  label: string;
  empty: string;
  itemKey: (item: Item) => string;
  renderItem: (item: Item) => ReactNode;
}

function MenuListInner<Item>(props: MenuListProps<Item>, ref: React.Ref<MenuHandle>) {
  const [selected, setSelected] = useState(0);
  // A new query starts the selection over at the top.
  const [shownItems, setShownItems] = useState(props.items);
  if (shownItems !== props.items) {
    setShownItems(props.items);
    setSelected(0);
  }

  const choose = (index: number) => {
    const item = props.items[index];
    if (item) props.command(item);
  };

  useImperativeHandle(ref, () => ({
    onKeyDown: ({ event }) => {
      const count = props.items.length;
      if (count === 0) return false;
      if (event.key === "ArrowDown") {
        setSelected((s) => (s + 1) % count);
        return true;
      }
      if (event.key === "ArrowUp") {
        setSelected((s) => (s + count - 1) % count);
        return true;
      }
      if (event.key === "Enter") {
        choose(selected);
        return true;
      }
      return false;
    },
  }));

  if (props.items.length === 0) {
    return (
      <div className="forma-popover px-3 py-2 text-[13px] text-muted-foreground">{props.empty}</div>
    );
  }
  return (
    <div
      role="listbox"
      aria-label={props.label}
      className="forma-popover max-h-80 w-64 overflow-y-auto p-1"
    >
      {props.items.map((item, index) => (
        <button
          key={props.itemKey(item)}
          type="button"
          role="option"
          aria-selected={index === selected}
          onMouseEnter={() => setSelected(index)}
          onMouseDown={(e) => {
            e.preventDefault();
            choose(index);
          }}
          className={cn(
            "flex min-h-8 w-full items-center gap-2 rounded-md px-2 py-1 text-left text-[13.5px]",
            index === selected && "bg-accent",
          )}
        >
          {props.renderItem(item)}
        </button>
      ))}
    </div>
  );
}

const MenuList = forwardRef(MenuListInner) as <Item>(
  props: MenuListProps<Item> & { ref?: React.Ref<MenuHandle> },
) => ReturnType<typeof MenuListInner>;

/** Suggestion `render` callbacks that show `MenuList` under the cursor. */
export function suggestionPopup<Item>(
  list: Pick<MenuListProps<Item>, "label" | "empty" | "itemKey" | "renderItem">,
) {
  return () => {
    let renderer: ReactRenderer<MenuHandle, MenuListProps<Item>> | null = null;
    let host: HTMLDivElement | null = null;
    const place = (props: SuggestionProps<Item>) => {
      const rect = props.clientRect?.();
      if (!host || !rect) return;
      host.style.left = `${rect.left}px`;
      host.style.top = `${rect.bottom + 6}px`;
    };
    return {
      onStart: (props: SuggestionProps<Item>) => {
        renderer = new ReactRenderer(MenuList as never, {
          props: { ...props, ...list },
          editor: props.editor,
        });
        host = document.createElement("div");
        host.style.position = "fixed";
        host.style.zIndex = "60";
        host.append(renderer.element);
        document.body.append(host);
        place(props);
      },
      onUpdate: (props: SuggestionProps<Item>) => {
        renderer?.updateProps({ ...props, ...list });
        place(props);
      },
      onKeyDown: (props: SuggestionKeyDownProps) => renderer?.ref?.onKeyDown(props) ?? false,
      onExit: () => {
        host?.remove();
        renderer?.destroy();
        host = null;
        renderer = null;
      },
    };
  };
}
