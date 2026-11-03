// Autosave for one open page (ARCHITECTURE §6.4). Framework-free so its
// timing can be unit tested.
//
// - Saves 750 ms after the last edit, and at least every 5 s while typing.
// - At most one save is in flight; edits made meanwhile are saved after it.
// - A conflict pauses saving until the user resolves it.
// - A failed save keeps the content marked unsaved and retries with backoff.
import type { SaveOutcome } from "@/lib/ipc/bindings/SaveOutcome";

export type SaveStatus = "saved" | "unsaved" | "saving" | "error" | "conflict";

export interface Conflict {
  /** What the editor holds (not saved). */
  mine: string;
  /** Body revision on disk now. */
  currentRev: string | null;
  /** What's on disk now, or null if the file is gone. */
  theirs: string | null;
}

export interface SaveControllerOptions {
  /** The body revision the editor started from (ADR-033). */
  baseRev: string | null;
  save: (body: string, baseRev: string | null) => Promise<SaveOutcome>;
  onStatus: (status: SaveStatus, error?: string) => void;
  onConflict: (conflict: Conflict) => void;
  onSaved?: (rev: string) => void;
  debounceMs?: number;
  maxWaitMs?: number;
  retryDelaysMs?: number[];
  now?: () => number;
}

export class SaveController {
  private baseRev: string | null;
  private readBody: (() => string) | null = null;
  private dirtySince: number | null = null;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private inFlight: Promise<void> | null = null;
  private paused = false;
  private failures = 0;
  private disposed = false;
  private readonly opts: Required<Omit<SaveControllerOptions, "onSaved">> &
    Pick<SaveControllerOptions, "onSaved">;

  constructor(options: SaveControllerOptions) {
    this.baseRev = options.baseRev;
    this.opts = {
      debounceMs: 750,
      maxWaitMs: 5000,
      retryDelaysMs: [2000, 5000, 10_000],
      now: () => Date.now(),
      ...options,
    };
  }

  get revision(): string | null {
    return this.baseRev;
  }

  get isDirty(): boolean {
    return this.dirtySince !== null;
  }

  get isPaused(): boolean {
    return this.paused;
  }

  /** Records an edit. `readBody` serializes the editor; it runs only when saving. */
  markDirty(readBody: () => string) {
    if (this.disposed) return;
    this.readBody = readBody;
    this.dirtySince ??= this.opts.now();
    if (this.paused) return;
    this.opts.onStatus("unsaved");
    const waited = this.opts.now() - this.dirtySince;
    this.schedule(Math.max(0, Math.min(this.opts.debounceMs, this.opts.maxWaitMs - waited)));
  }

  /**
   * Captures the current body so a save that runs after the editor is gone
   * (page switch, quit) still has the right content.
   */
  freezeBody() {
    if (!this.readBody || !this.isDirty) return;
    const snapshot = this.readBody();
    this.readBody = () => snapshot;
  }

  /** Saves pending edits now and waits for every save in flight. */
  async flush(): Promise<void> {
    this.clearTimer();
    if (this.inFlight) await this.inFlight;
    if (this.isDirty && !this.paused) await this.run();
  }

  /** Stops saving until `resume` (a conflict is open, or the page's file is gone). */
  pause() {
    this.paused = true;
    this.clearTimer();
  }

  /** What a save would write now, or null when nothing is unsaved. */
  pendingBody(): string | null {
    return this.isDirty && this.readBody ? this.readBody() : null;
  }

  /** Continues after a conflict, from the revision now on disk. */
  resume(baseRev: string | null, stillDirty = false) {
    this.paused = false;
    this.baseRev = baseRev;
    if (!stillDirty) this.dirtySince = null;
    this.opts.onStatus(stillDirty ? "unsaved" : "saved");
    if (stillDirty) this.schedule(this.opts.debounceMs);
  }

  /** Adopts content reloaded from disk (the editor had no unsaved edits). */
  reset(baseRev: string | null) {
    this.clearTimer();
    this.baseRev = baseRev;
    this.dirtySince = null;
    this.paused = false;
    this.opts.onStatus("saved");
  }

  dispose() {
    this.disposed = true;
    this.clearTimer();
  }

  private schedule(delay: number) {
    this.clearTimer();
    this.timer = setTimeout(() => void this.run(), delay);
  }

  private clearTimer() {
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
  }

  private run(): Promise<void> {
    if (this.inFlight) {
      // Save again once the current save finishes.
      return this.inFlight.then(() => (this.isDirty && !this.paused ? this.run() : undefined));
    }
    if (!this.isDirty || this.paused || !this.readBody) return Promise.resolve();
    const body = this.readBody();
    const startedDirtyAt = this.dirtySince;
    this.dirtySince = null;
    this.opts.onStatus("saving");
    this.inFlight = this.opts
      .save(body, this.baseRev)
      .then((outcome) => {
        this.failures = 0;
        if (outcome.outcome === "saved") {
          this.baseRev = outcome.body_rev;
          this.opts.onSaved?.(outcome.body_rev);
          this.opts.onStatus(this.isDirty ? "unsaved" : "saved");
          if (this.isDirty && !this.disposed) this.schedule(this.opts.debounceMs);
        } else {
          this.paused = true;
          this.dirtySince ??= startedDirtyAt;
          this.opts.onStatus("conflict");
          this.opts.onConflict({
            mine: body,
            currentRev: outcome.current_body_rev,
            theirs: outcome.current_body,
          });
        }
      })
      .catch((error: unknown) => {
        this.dirtySince ??= startedDirtyAt ?? this.opts.now();
        const delays = this.opts.retryDelaysMs;
        const delay = delays[Math.min(this.failures, delays.length - 1)] ?? 10_000;
        this.failures += 1;
        this.opts.onStatus("error", error instanceof Error ? error.message : String(error));
        if (!this.disposed) this.schedule(delay);
      })
      .finally(() => {
        this.inFlight = null;
      });
    return this.inFlight;
  }
}
