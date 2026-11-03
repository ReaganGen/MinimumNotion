// Facts about AI providers the UI shows. Mirrors crates/forma-ai/src/presets.rs;
// Rust enforces the same rules before anything is sent.
import type { AiSettings } from "@/lib/ipc/bindings/AiSettings";
import type { ProviderConfig } from "@/lib/ipc/bindings/ProviderConfig";
import type { ProviderKind } from "@/lib/ipc/bindings/ProviderKind";

export const PROVIDER_KINDS: { kind: ProviderKind; label: string; hint: string }[] = [
  { kind: "anthropic", label: "Anthropic", hint: "Claude models, with an API key" },
  { kind: "openai", label: "OpenAI", hint: "GPT models, with an API key" },
  { kind: "ollama", label: "Ollama", hint: "Local models on this Mac" },
  { kind: "lmstudio", label: "LM Studio", hint: "Local models on this Mac" },
  { kind: "openai_compatible", label: "OpenAI-compatible", hint: "Any server with the OpenAI API" },
];

/** Cloud APIs refuse every request without a key; local servers usually need none. */
export const KEY_REQUIRED: Record<ProviderKind, boolean> = {
  anthropic: true,
  openai: true,
  openai_compatible: false,
  ollama: false,
  lmstudio: false,
};

function parse(url: string): URL | null {
  try {
    return new URL(url);
  } catch {
    return null;
  }
}

/** Whether requests to `url` stay on this Mac (a loopback address). */
export function isLocalUrl(url: string): boolean {
  const host = parse(url)?.hostname.toLowerCase();
  if (!host) return false;
  return (
    host === "localhost" ||
    host.endsWith(".localhost") ||
    host === "[::1]" ||
    /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(host)
  );
}

export function hostOf(url: string): string {
  return parse(url)?.host ?? url;
}

/** Plain http to another machine: requests, and any key, travel unencrypted. */
export function isUnencryptedRemote(url: string): boolean {
  return parse(url)?.protocol === "http:" && !isLocalUrl(url);
}

export type Readiness =
  | { state: "disabled" }
  | { state: "no-provider" }
  | { state: "no-model"; provider: ProviderConfig }
  | { state: "no-key"; provider: ProviderConfig }
  | { state: "needs-agreement"; provider: ProviderConfig }
  | { state: "ready"; provider: ProviderConfig };

/** Whether a chat can be sent now, and if not, what's missing. */
export function readiness(ai: AiSettings): Readiness {
  if (!ai.enabled) return { state: "disabled" };
  const provider = ai.providers.find((p) => p.id === ai.active_provider);
  if (!provider) return { state: "no-provider" };
  if (!provider.model) return { state: "no-model", provider };
  if (KEY_REQUIRED[provider.kind] && !provider.has_credential) return { state: "no-key", provider };
  if (!isLocalUrl(provider.base_url) && !(provider.id in ai.cloud_consent)) {
    return { state: "needs-agreement", provider };
  }
  return { state: "ready", provider };
}

/** Where a request goes, in words. */
export function destinationLabel(provider: ProviderConfig): string {
  return isLocalUrl(provider.base_url)
    ? "Stays on this Mac"
    : `Sent to ${provider.name} (${hostOf(provider.base_url)})`;
}
