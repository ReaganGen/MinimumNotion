// The IPC contract: every command the client calls exists in the shell and
// takes exactly the arguments the client sends. Tauri maps a Rust parameter
// `base_body_rev` to the JavaScript key `baseBodyRev`; a mismatch only shows up
// at runtime, so this test reads both sides and compares them.
import { describe, expect, it } from "vitest";
import aiCommands from "../../../src-tauri/src/ai_commands.rs?raw";
import settingsCommands from "../../../src-tauri/src/commands.rs?raw";
import shell from "../../../src-tauri/src/main.rs?raw";
import workspaceCommands from "../../../src-tauri/src/workspace_commands.rs?raw";
import client from "./client.ts?raw";

/** Parameters Tauri fills in itself, not from the call's arguments. */
const INJECTED = /^(?:State|AppHandle|WebviewWindow|Window|Request)\b/;

const camel = (snake: string) => snake.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());

function rustCommands(source: string): Map<string, string[]> {
  const commands = new Map<string, string[]>();
  const pattern =
    /#\[tauri::command[^\]]*\]\s*(?:pub )?(?:async )?fn (\w+)(?:<[^>]*>)?\(([^)]*)\)/g;
  for (const [, name, params] of source.matchAll(pattern)) {
    const args = (params ?? "")
      .split(",")
      .map((p) => p.trim())
      .filter(Boolean)
      .map((p) => p.split(":").map((s) => s.trim()))
      .filter(([, type]) => type !== undefined && !INJECTED.test(type))
      .map(([param]) => camel(param ?? ""));
    commands.set(name ?? "", args);
  }
  return commands;
}

function clientCalls(source: string): Map<string, string[]> {
  const calls = new Map<string, string[]>();
  for (const [, name, args] of source.matchAll(/call<[^>]*>\(\s*"(\w+)"(?:,\s*\{([^}]*)\})?/g)) {
    const keys = (args ?? "")
      .split(",")
      .map((a) => a.trim().split(":")[0]?.trim())
      .filter((a): a is string => Boolean(a));
    calls.set(name ?? "", keys);
  }
  return calls;
}

describe("IPC contract", () => {
  const rust = new Map(
    [shell, settingsCommands, workspaceCommands, aiCommands].flatMap((source) => [
      ...rustCommands(source),
    ]),
  );
  const calls = clientCalls(client);

  it("finds the commands on both sides", () => {
    expect(rust.size).toBeGreaterThan(25);
    expect(calls.size).toBeGreaterThan(25);
  });

  it("sends exactly the arguments each command takes", () => {
    for (const [name, keys] of calls) {
      expect(rust.has(name), `${name} is not a shell command`).toBe(true);
      expect([...keys].sort(), name).toEqual([...(rust.get(name) ?? [])].sort());
    }
  });
});
