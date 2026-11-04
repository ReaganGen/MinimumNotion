import { type FormEvent, useEffect, useId, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import { answerBookmarkDialog, useBookmarkDialog } from "@/features/editor/bookmarkDialog";
import { bookmarkUrl } from "@/features/editor/cards";

const inputClass =
  "h-8 w-full min-w-0 rounded-md bg-muted px-2 text-[13px] outline-none placeholder:text-faint-foreground focus:ring-1 focus:ring-ring";

/**
 * Adds or edits a bookmark (ADR-049). Nothing is fetched: the title and the
 * description are what the user types.
 */
export function BookmarkDialog() {
  const request = useBookmarkDialog((s) => s.request);
  const [fields, setFields] = useState({ url: "", title: "", description: "" });
  const [tried, setTried] = useState(false);
  const ids = useId();

  useEffect(() => {
    if (request) setFields(request.initial);
    setTried(false);
  }, [request]);

  const valid = bookmarkUrl(fields.url) !== null;
  const editing = request?.initial.url !== "";

  function submit(event: FormEvent) {
    event.preventDefault();
    setTried(true);
    if (valid) answerBookmarkDialog(fields);
  }

  return (
    <Dialog open={request !== null} onOpenChange={(open) => !open && answerBookmarkDialog(null)}>
      <DialogContent className="max-w-[440px] gap-0 p-0 sm:max-w-[440px]">
        <form onSubmit={submit}>
          <div className="border-b px-5 py-4">
            <DialogTitle className="text-[15px] font-semibold">
              {editing ? "Edit bookmark" : "Add a bookmark"}
            </DialogTitle>
            <DialogDescription className="mt-1 text-[13px] text-muted-foreground">
              Diaspar doesn't visit the page. The card shows what you type here.
            </DialogDescription>
          </div>
          <div className="flex flex-col gap-3 px-5 py-4">
            <label
              className="flex flex-col gap-1 text-[12.5px] text-muted-foreground"
              htmlFor={`${ids}-url`}
            >
              Address
              <input
                id={`${ids}-url`}
                className={inputClass}
                value={fields.url}
                placeholder="https://example.com"
                autoComplete="off"
                spellCheck={false}
                aria-invalid={tried && !valid}
                aria-describedby={tried && !valid ? `${ids}-error` : undefined}
                onChange={(e) => setFields({ ...fields, url: e.target.value })}
              />
            </label>
            {tried && !valid && (
              <p id={`${ids}-error`} className="-mt-1 text-[12.5px] text-destructive">
                Use a web address that starts with http or https.
              </p>
            )}
            <label
              className="flex flex-col gap-1 text-[12.5px] text-muted-foreground"
              htmlFor={`${ids}-title`}
            >
              Title
              <input
                id={`${ids}-title`}
                className={inputClass}
                value={fields.title}
                placeholder="The address, if left empty"
                onChange={(e) => setFields({ ...fields, title: e.target.value })}
              />
            </label>
            <label
              className="flex flex-col gap-1 text-[12.5px] text-muted-foreground"
              htmlFor={`${ids}-description`}
            >
              Description (optional)
              <input
                id={`${ids}-description`}
                className={inputClass}
                value={fields.description}
                onChange={(e) => setFields({ ...fields, description: e.target.value })}
              />
            </label>
          </div>
          <DialogFooter className="border-t px-5 py-3">
            <Button type="button" variant="ghost" onClick={() => answerBookmarkDialog(null)}>
              Cancel
            </Button>
            <Button type="submit">{editing ? "Save" : "Add bookmark"}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
