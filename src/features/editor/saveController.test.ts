import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SaveOutcome } from "@/lib/ipc/bindings/SaveOutcome";
import { type Conflict, SaveController, type SaveStatus } from "./saveController";

const saved = (rev: string): SaveOutcome => ({
  outcome: "saved",
  rev: `file-${rev}`,
  body_rev: rev,
  updated_at: "now",
});

function setup(save: (body: string, base: string | null) => Promise<SaveOutcome>) {
  const statuses: SaveStatus[] = [];
  const conflicts: Conflict[] = [];
  const controller = new SaveController({
    baseRev: "r0",
    save,
    onStatus: (s) => statuses.push(s),
    onConflict: (c) => conflicts.push(c),
    now: () => Date.now(),
  });
  return { controller, statuses, conflicts };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("SaveController", () => {
  it("saves once, 750 ms after the last edit", async () => {
    const save = vi.fn(async () => saved("r1"));
    const { controller } = setup(save);
    controller.markDirty(() => "a");
    await vi.advanceTimersByTimeAsync(500);
    controller.markDirty(() => "ab");
    await vi.advanceTimersByTimeAsync(500);
    expect(save).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(300);
    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith("ab", "r0");
    expect(controller.revision).toBe("r1");
    expect(controller.isDirty).toBe(false);
  });

  it("saves at least every 5 s during continuous typing", async () => {
    const save = vi.fn(async () => saved("r1"));
    const { controller } = setup(save);
    for (let t = 0; t < 5200; t += 200) {
      controller.markDirty(() => `text ${t}`);
      await vi.advanceTimersByTimeAsync(200);
    }
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("never overlaps saves and saves edits made during a save", async () => {
    let finish: (o: SaveOutcome) => void = () => {};
    const save = vi
      .fn<(body: string, base: string | null) => Promise<SaveOutcome>>()
      .mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)))
      .mockImplementation(async () => saved("r2"));
    const { controller } = setup(save);
    controller.markDirty(() => "first");
    await vi.advanceTimersByTimeAsync(750);
    expect(save).toHaveBeenCalledTimes(1);

    controller.markDirty(() => "second");
    await vi.advanceTimersByTimeAsync(2000);
    expect(save).toHaveBeenCalledTimes(1); // still waiting for the first save

    finish(saved("r1"));
    await vi.advanceTimersByTimeAsync(800);
    expect(save).toHaveBeenCalledTimes(2);
    expect(save).toHaveBeenLastCalledWith("second", "r1");
    expect(controller.revision).toBe("r2");
  });

  it("flush saves immediately and waits", async () => {
    const save = vi.fn(async () => saved("r1"));
    const { controller } = setup(save);
    controller.markDirty(() => "now");
    await controller.flush();
    expect(save).toHaveBeenCalledWith("now", "r0");
    expect(controller.isDirty).toBe(false);
  });

  it("pauses on a conflict and keeps the edit unsaved", async () => {
    const save = vi.fn(
      async (): Promise<SaveOutcome> => ({
        outcome: "conflict",
        current_body_rev: "x1",
        current_body: "theirs",
      }),
    );
    const { controller, statuses, conflicts } = setup(save);
    controller.markDirty(() => "mine");
    await vi.advanceTimersByTimeAsync(750);
    expect(conflicts).toEqual([{ mine: "mine", currentRev: "x1", theirs: "theirs" }]);
    expect(statuses.at(-1)).toBe("conflict");
    expect(controller.isPaused).toBe(true);
    expect(controller.isDirty).toBe(true);

    // Further edits don't save while paused.
    controller.markDirty(() => "mine 2");
    await vi.advanceTimersByTimeAsync(10_000);
    await controller.flush();
    expect(save).toHaveBeenCalledTimes(1);

    controller.resume("x2", true);
    await vi.advanceTimersByTimeAsync(750);
    expect(save).toHaveBeenLastCalledWith("mine 2", "x2");
  });

  it("retries a failed save and stays unsaved meanwhile", async () => {
    const save = vi
      .fn<(body: string, base: string | null) => Promise<SaveOutcome>>()
      .mockRejectedValueOnce(new Error("disk full"))
      .mockResolvedValue(saved("r1"));
    const { controller, statuses } = setup(save);
    controller.markDirty(() => "text");
    await vi.advanceTimersByTimeAsync(750);
    expect(statuses.at(-1)).toBe("error");
    expect(controller.isDirty).toBe(true);
    await vi.advanceTimersByTimeAsync(2000);
    expect(save).toHaveBeenCalledTimes(2);
    expect(statuses.at(-1)).toBe("saved");
    expect(controller.isDirty).toBe(false);
  });

  it("does nothing after dispose", async () => {
    const save = vi.fn(async () => saved("r1"));
    const { controller } = setup(save);
    controller.markDirty(() => "a");
    controller.dispose();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(save).not.toHaveBeenCalled();
  });
});
