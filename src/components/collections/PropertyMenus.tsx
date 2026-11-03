// Editing a collection's schema (DATA_MODEL §6.4): add, rename, retype,
// hide and delete properties, and edit select options. Deleting a property
// leaves its values in the entries' files, and the dialog says so.
import { EyeOff, Palette, Pencil, Plus, Trash2 } from "lucide-react";
import { type FormEvent, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { Property } from "@/lib/ipc/bindings/Property";
import type { PropertyType } from "@/lib/ipc/bindings/PropertyType";
import type { SchemaOp } from "@/lib/ipc/bindings/SchemaOp";
import { OptionChip, TYPE_ICONS, TYPE_LABELS } from "./cells";

export const COLORS = [
  "gray",
  "brown",
  "orange",
  "yellow",
  "green",
  "blue",
  "purple",
  "pink",
  "red",
] as const;

const TYPES = Object.keys(TYPE_LABELS) as PropertyType[];

type Apply = (op: SchemaOp) => Promise<boolean>;

/** The menu on a property's column header. */
export function PropertyHeader({
  property,
  editable,
  apply,
  onHide,
}: {
  property: Property;
  editable: boolean;
  apply: Apply;
  onHide: () => void;
}) {
  const [renaming, setRenaming] = useState(false);
  const [name, setName] = useState(property.name);
  const [dialog, setDialog] = useState<"options" | "delete" | null>(null);
  const Icon = TYPE_ICONS[property.type];

  async function rename(event?: FormEvent) {
    event?.preventDefault();
    setRenaming(false);
    if (name.trim() && name.trim() !== property.name) {
      const ok = await apply({ op: "rename_property", property_id: property.id, name });
      if (!ok) setName(property.name);
    } else {
      setName(property.name);
    }
  }

  if (renaming) {
    return (
      <form onSubmit={rename} className="flex h-full items-center">
        <input
          // biome-ignore lint/a11y/noAutofocus: renaming starts from the menu
          autoFocus
          aria-label="Property name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => void rename()}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setName(property.name);
              setRenaming(false);
            }
          }}
          className="h-6 w-full rounded-sm bg-background px-1 text-[13px] outline-none ring-1 ring-ring"
        />
      </form>
    );
  }

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild disabled={!editable}>
          <button
            type="button"
            className="flex h-full w-full min-w-0 items-center gap-1.5 text-left disabled:cursor-default"
          >
            <Icon className="size-3.5 shrink-0 text-faint-foreground" aria-hidden />
            <span className="truncate">{property.name}</span>
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-52">
          <DropdownMenuItem onSelect={() => setRenaming(true)}>
            <Pencil aria-hidden /> Rename
          </DropdownMenuItem>
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <Icon aria-hidden /> Type: {TYPE_LABELS[property.type]}
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuRadioGroup
                value={property.type}
                onValueChange={(type) =>
                  void apply({
                    op: "set_property_type",
                    property_id: property.id,
                    type: type as PropertyType,
                  })
                }
              >
                {TYPES.map((type) => (
                  <DropdownMenuRadioItem key={type} value={type}>
                    {TYPE_LABELS[type]}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
          {(property.type === "select" || property.type === "multi_select") && (
            <DropdownMenuItem onSelect={() => setDialog("options")}>
              <Palette aria-hidden /> Edit options
            </DropdownMenuItem>
          )}
          <DropdownMenuItem onSelect={onHide}>
            <EyeOff aria-hidden /> Hide in this view
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => setDialog("delete")}>
            <Trash2 aria-hidden /> Delete property
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <OptionsDialog
        property={property}
        open={dialog === "options"}
        onClose={() => setDialog(null)}
        apply={apply}
      />
      <DeletePropertyDialog
        property={property}
        open={dialog === "delete"}
        onClose={() => setDialog(null)}
        apply={apply}
      />
    </>
  );
}

function DeletePropertyDialog({
  property,
  open,
  onClose,
  apply,
}: {
  property: Property;
  open: boolean;
  onClose: () => void;
  apply: Apply;
}) {
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="max-w-[420px] sm:max-w-[420px]">
        <DialogTitle className="text-[15px] font-semibold">Delete “{property.name}”?</DialogTitle>
        <DialogDescription className="text-[13px] leading-5 text-muted-foreground">
          The column goes away. Each page keeps its value in its file, and the values come back if
          you add a property with the same name.
        </DialogDescription>
        <div className="mt-2 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-[13px] hover:bg-accent"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => {
              onClose();
              void apply({ op: "remove_property", property_id: property.id });
            }}
            className="rounded-md bg-destructive px-3 py-1.5 text-[13px] text-primary-foreground hover:bg-destructive/90"
          >
            Delete property
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function OptionsDialog({
  property,
  open,
  onClose,
  apply,
}: {
  property: Property;
  open: boolean;
  onClose: () => void;
  apply: Apply;
}) {
  const [adding, setAdding] = useState("");
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="max-w-[440px] gap-0 p-0 sm:max-w-[440px]">
        <div className="border-b px-5 py-4">
          <DialogTitle className="text-[15px] font-semibold">
            Options for “{property.name}”
          </DialogTitle>
          <DialogDescription className="mt-1 text-[13px] text-muted-foreground">
            Renaming an option updates every page that uses it.
          </DialogDescription>
        </div>
        <ul className="max-h-[50vh] overflow-y-auto p-2">
          {property.options.map((option) => (
            <li key={option.id} className="flex items-center gap-2 rounded-md px-2 py-1">
              <OptionNameInput
                name={option.name}
                onRename={(name) =>
                  apply({
                    op: "rename_option",
                    property_id: property.id,
                    option_id: option.id,
                    name,
                  })
                }
              />
              <select
                aria-label={`Color of ${option.name}`}
                value={option.color}
                onChange={(e) =>
                  void apply({
                    op: "set_option_color",
                    property_id: property.id,
                    option_id: option.id,
                    color: e.target.value,
                  })
                }
                className="h-7 rounded-sm bg-muted px-1 text-[12.5px] outline-none"
              >
                {COLORS.map((color) => (
                  <option key={color} value={color}>
                    {color}
                  </option>
                ))}
              </select>
              <OptionChip name={option.name} color={option.color} />
              <button
                type="button"
                aria-label={`Remove ${option.name}`}
                onClick={() =>
                  void apply({
                    op: "remove_option",
                    property_id: property.id,
                    option_id: option.id,
                  })
                }
                className="ml-auto rounded-sm p-1 text-muted-foreground hover:bg-accent"
              >
                <Trash2 className="size-3.5" aria-hidden />
              </button>
            </li>
          ))}
          {property.options.length === 0 && (
            <li className="px-2 py-1 text-[13px] text-muted-foreground">No options yet.</li>
          )}
        </ul>
        <form
          className="flex gap-2 border-t px-4 py-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (!adding.trim()) return;
            void apply({ op: "add_option", property_id: property.id, name: adding }).then(
              (ok) => ok && setAdding(""),
            );
          }}
        >
          <input
            aria-label="New option"
            placeholder="New option…"
            value={adding}
            onChange={(e) => setAdding(e.target.value)}
            className="h-8 min-w-0 flex-1 rounded-md bg-muted px-2 text-[13px] outline-none"
          />
          <button
            type="submit"
            className="rounded-md px-3 text-[13px] text-foreground shadow-popover hover:bg-accent"
          >
            Add
          </button>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function OptionNameInput({
  name,
  onRename,
}: {
  name: string;
  onRename: (name: string) => Promise<boolean>;
}) {
  const [value, setValue] = useState(name);
  const commit = () => {
    if (value.trim() && value.trim() !== name) {
      void onRename(value).then((ok) => !ok && setValue(name));
    } else {
      setValue(name);
    }
  };
  return (
    <input
      aria-label={`Name of ${name}`}
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
      className="h-7 w-36 rounded-sm bg-transparent px-1 text-[13px] outline-none focus:ring-1 focus:ring-ring"
    />
  );
}

/** "+" at the end of the header row: a name and a type. */
export function AddPropertyButton({ apply }: { apply: Apply }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [type, setType] = useState<PropertyType>("text");

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!name.trim()) return;
    if (await apply({ op: "add_property", name, type })) {
      setName("");
      setType("text");
      setOpen(false);
    }
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label="Add a property"
          className="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          <Plus className="size-4" aria-hidden />
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64">
        <form onSubmit={submit} className="flex flex-col gap-2">
          <input
            // biome-ignore lint/a11y/noAutofocus: the popover opens to name the property
            autoFocus
            aria-label="Property name"
            placeholder="Property name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="h-8 rounded-md bg-muted px-2 text-[13px] outline-none"
          />
          <select
            aria-label="Type"
            value={type}
            onChange={(e) => setType(e.target.value as PropertyType)}
            className="h-8 rounded-md bg-muted px-1.5 text-[13px] outline-none"
          >
            {TYPES.map((t) => (
              <option key={t} value={t}>
                {TYPE_LABELS[t]}
              </option>
            ))}
          </select>
          <button
            type="submit"
            disabled={!name.trim()}
            className="h-8 rounded-md bg-primary text-[13px] text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            Add property
          </button>
        </form>
      </PopoverContent>
    </Popover>
  );
}
