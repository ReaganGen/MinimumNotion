import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ResizeHandle } from "./ResizeHandle";

function setup(edge: "left" | "right") {
  const onResize = vi.fn();
  const onDraggingChange = vi.fn();
  render(
    <ResizeHandle
      edge={edge}
      label="Resize"
      width={260}
      min={200}
      max={480}
      defaultWidth={250}
      onResize={onResize}
      onDraggingChange={onDraggingChange}
    />,
  );
  return { handle: screen.getByRole("separator", { name: "Resize" }), onResize, onDraggingChange };
}

describe("ResizeHandle", () => {
  it("exposes its value to assistive technology", () => {
    const { handle } = setup("right");
    expect(handle).toHaveAttribute("aria-valuenow", "260");
    expect(handle).toHaveAttribute("aria-valuemin", "200");
    expect(handle).toHaveAttribute("aria-valuemax", "480");
  });

  it("resizes with the keyboard, mirrored for a left-edge handle", () => {
    const right = setup("right");
    fireEvent.keyDown(right.handle, { key: "ArrowRight" });
    expect(right.onResize).toHaveBeenLastCalledWith(276);
    fireEvent.keyDown(right.handle, { key: "Home" });
    expect(right.onResize).toHaveBeenLastCalledWith(200);
    fireEvent.keyDown(right.handle, { key: "End" });
    expect(right.onResize).toHaveBeenLastCalledWith(480);
  });

  it("grows a left-edge panel when dragged left", () => {
    const { handle, onResize, onDraggingChange } = setup("left");
    fireEvent.pointerDown(handle, { button: 0, clientX: 500, pointerId: 1 });
    expect(onDraggingChange).toHaveBeenLastCalledWith(true);
    fireEvent.pointerMove(handle, { clientX: 450, pointerId: 1 });
    expect(onResize).toHaveBeenLastCalledWith(310);
    fireEvent.pointerUp(handle, { clientX: 450, pointerId: 1 });
    expect(onDraggingChange).toHaveBeenLastCalledWith(false);
  });

  it("ignores pointer moves when not dragging", () => {
    const { handle, onResize } = setup("right");
    fireEvent.pointerMove(handle, { clientX: 900 });
    expect(onResize).not.toHaveBeenCalled();
  });

  it("resets to the default width on double-click", () => {
    const { handle, onResize } = setup("right");
    fireEvent.doubleClick(handle);
    expect(onResize).toHaveBeenLastCalledWith(250);
  });
});
