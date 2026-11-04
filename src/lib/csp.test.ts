// The webview's Content Security Policy (ARCHITECTURE §6.6, ADR-031, ADR-049).
import { describe, expect, it } from "vitest";

const raw = Object.values(
  import.meta.glob("/src-tauri/tauri.conf.json", { query: "?raw", import: "default", eager: true }),
)[0] as string;
const security = JSON.parse(raw).app.security as Record<"csp" | "devCsp", Record<string, string>>;

describe.each(["csp", "devCsp"] as const)("%s", (name) => {
  const policy = security[name];

  it("loads audio and video from the workspace only", () => {
    expect(policy["media-src"]).toBe("'self' diaspar-asset:");
  });

  it("keeps images to the workspace and https (ADR-031)", () => {
    expect(policy["img-src"]).toBe("'self' diaspar-asset: https:");
  });

  it("allows no frames by URL, plugins or form posts", () => {
    // HTML embeds are srcdoc frames with an empty sandbox; nothing is framed by URL.
    expect(policy["frame-src"]).toBe("'none'");
    expect(policy["object-src"]).toBe("'none'");
    expect(policy["form-action"]).toBe("'none'");
    expect(policy["base-uri"]).toBe("'none'");
  });

  it("talks only to the app itself", () => {
    expect(policy["connect-src"]).not.toMatch(/https?:\/\/(?!ipc\.localhost|localhost:1420)/);
    expect(policy["default-src"]).toBe("'self'");
  });
});

it("runs no inline scripts in the built app", () => {
  expect(security.csp["script-src"]).toBe("'self'");
});
