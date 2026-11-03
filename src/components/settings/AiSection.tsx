// Settings → AI: turn AI on, set up providers, store keys in the Keychain,
// pick a model and test the connection (IMPLEMENTATION_PLAN §7.1 task 4).
import { Cloud, KeyRound, Laptop, Plus } from "lucide-react";
import { type ReactNode, useEffect, useId, useState } from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  hostOf,
  isLocalUrl,
  isUnencryptedRemote,
  KEY_REQUIRED,
  PROVIDER_KINDS,
} from "@/features/ai/providers";
import { useSettingsStore } from "@/features/settings/settingsStore";
import type { AppSettings } from "@/lib/ipc/bindings/AppSettings";
import type { ConnectionReport } from "@/lib/ipc/bindings/ConnectionReport";
import type { GenerationOptions } from "@/lib/ipc/bindings/GenerationOptions";
import type { ModelCapabilities } from "@/lib/ipc/bindings/ModelCapabilities";
import type { ModelInfo } from "@/lib/ipc/bindings/ModelInfo";
import type { ProviderConfig } from "@/lib/ipc/bindings/ProviderConfig";
import { ipc } from "@/lib/ipc/client";
import { cn } from "@/lib/utils";

/** Runs an AI settings command and takes the saved AI settings, or reports why it failed. */
async function save(
  command: Promise<AppSettings>,
  onError: (message: string | null) => void,
): Promise<boolean> {
  try {
    useSettingsStore.getState().applyAi(await command);
    onError(null);
    return true;
  } catch (error) {
    onError((error as Error).message);
    return false;
  }
}

const buttonClass =
  "rounded-md px-3 py-1.5 text-[13px] text-foreground shadow-popover hover:bg-accent disabled:opacity-50";
const inputClass =
  "h-8 w-full min-w-0 rounded-md bg-muted px-2 text-[13px] outline-none placeholder:text-faint-foreground focus:ring-1 focus:ring-ring";

export function AiSection() {
  const ai = useSettingsStore((s) => s.settings.ai);
  const readOnly = useSettingsStore((s) => s.readOnlyReason);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(ai.active_provider);
  const selected =
    ai.providers.find((p) => p.id === editing) ??
    ai.providers.find((p) => p.id === ai.active_provider) ??
    ai.providers[0];

  async function add(kind: ProviderConfig["kind"]) {
    const before = new Set(ai.providers.map((p) => p.id));
    const ok = await save(ipc.aiAddProvider(kind), setError);
    if (ok) {
      const added = useSettingsStore
        .getState()
        .settings.ai.providers.find((p) => !before.has(p.id));
      if (added) setEditing(added.id);
    }
  }

  return (
    <section aria-labelledby="settings-ai">
      <h2 id="settings-ai" className="text-[15px] font-semibold">
        AI
      </h2>
      <div className="mt-5 flex items-center justify-between gap-6 border-t pt-4">
        <div>
          <p id="ai-enabled-label" className="text-[14px]">
            Use AI
          </p>
          <p className="text-[13px] leading-5 text-muted-foreground">
            Off by default. While it's off, Forma never sends anything anywhere.
          </p>
        </div>
        <Switch
          labelledBy="ai-enabled-label"
          checked={ai.enabled}
          disabled={readOnly !== null}
          onChange={(enabled) => void save(ipc.aiSetEnabled(enabled), setError)}
        />
      </div>
      {readOnly && <p className="mt-3 text-[13px] text-muted-foreground">{readOnly}</p>}
      {error && (
        <p role="alert" className="mt-3 text-[13px] text-destructive">
          {error}
        </p>
      )}

      {ai.enabled && (
        <div className="mt-6 border-t pt-4">
          <div className="flex items-center justify-between">
            <p className="text-[14px]">Providers</p>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[13px] text-muted-foreground hover:bg-accent hover:text-foreground"
                >
                  <Plus className="size-3.5" aria-hidden /> Add provider
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-64">
                {PROVIDER_KINDS.map(({ kind, label, hint }) => (
                  <DropdownMenuItem key={kind} onSelect={() => void add(kind)}>
                    <div>
                      <p className="text-[13px]">{label}</p>
                      <p className="text-[12px] text-muted-foreground">{hint}</p>
                    </div>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          {ai.providers.length === 0 ? (
            <p className="mt-2 text-[13px] text-muted-foreground">
              Add OpenAI or Anthropic with an API key, or a model running on this Mac with Ollama or
              LM Studio.
            </p>
          ) : (
            <ProviderList
              providers={ai.providers}
              active={ai.active_provider}
              selected={selected?.id ?? null}
              onSelect={setEditing}
              onActivate={(id) => void save(ipc.aiSetActiveProvider(id), setError)}
            />
          )}
          {selected && <ProviderEditor key={selected.id} provider={selected} />}
        </div>
      )}
    </section>
  );
}

function Switch({
  checked,
  onChange,
  labelledBy,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  labelledBy: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-labelledby={labelledBy}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative h-5 w-9 shrink-0 rounded-full transition-colors disabled:opacity-50",
        checked ? "bg-primary" : "bg-muted shadow-[inset_0_0_0_1px_var(--border)]",
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 left-0.5 size-4 rounded-full bg-background shadow-sm transition-transform",
          checked && "translate-x-4",
        )}
      />
    </button>
  );
}

function ProviderList({
  providers,
  active,
  selected,
  onSelect,
  onActivate,
}: {
  providers: ProviderConfig[];
  active: string | null;
  selected: string | null;
  onSelect: (id: string) => void;
  onActivate: (id: string) => void;
}) {
  const group = useId();
  return (
    <ul className="mt-2 flex flex-col gap-0.5">
      {providers.map((p) => {
        const Icon = isLocalUrl(p.base_url) ? Laptop : Cloud;
        return (
          <li
            key={p.id}
            className={cn(
              "flex items-center gap-2 rounded-md px-2 py-1.5",
              p.id === selected && "bg-selected",
            )}
          >
            <input
              type="radio"
              name={group}
              checked={p.id === active}
              onChange={() => onActivate(p.id)}
              aria-label={`Use ${p.name}`}
              className="size-3.5 accent-(--primary)"
            />
            <button
              type="button"
              onClick={() => onSelect(p.id)}
              className="flex min-w-0 flex-1 items-center gap-2 text-left"
            >
              <Icon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
              <span className="truncate text-[13px]">{p.name}</span>
              <span className="truncate text-[12px] text-muted-foreground">
                {p.model || "No model chosen"}
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

type Draft = { name: string; base_url: string; model: string; options: GenerationOptions };

const draftOf = (p: ProviderConfig): Draft => ({
  name: p.name,
  base_url: p.base_url,
  model: p.model,
  options: p.options,
});

function ProviderEditor({ provider }: { provider: ProviderConfig }) {
  const [draft, setDraft] = useState<Draft>(() => draftOf(provider));
  const [error, setError] = useState<string | null>(null);
  const [models, setModels] = useState<ModelInfo[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [report, setReport] = useState<ConnectionReport | "testing" | null>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const ids = useId();
  const listId = `${ids}-models`;

  /** Saves the draft (or `next`) when it differs from what's stored. */
  async function commit(next: Draft = draft) {
    setDraft(next);
    const stored = draftOf(provider);
    if (JSON.stringify(next) === JSON.stringify(stored)) return;
    const ok = await save(ipc.aiUpdateProvider(provider.id, next), setError);
    if (!ok) setDraft(stored);
    setReport(null);
  }

  async function loadModels() {
    setLoading(true);
    try {
      setModels(await ipc.aiListModels(provider.id));
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  }

  async function test() {
    setReport("testing");
    try {
      setReport(await ipc.aiTestConnection(provider.id));
    } catch (e) {
      setReport({ ok: false, message: (e as Error).message, error: null });
    }
  }

  // Token counts are whole numbers; only the temperature has decimals.
  const setOption = (key: keyof GenerationOptions, value: number | null) => {
    const v = value === null || key === "temperature" ? value : Math.round(value);
    void commit({ ...draft, options: { ...draft.options, [key]: v } });
  };
  const local = isLocalUrl(provider.base_url);

  return (
    <div className="mt-4 flex flex-col gap-4 rounded-lg px-4 py-4 shadow-popover">
      <Field label="Name" id={`${ids}-name`}>
        <input
          id={`${ids}-name`}
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          onBlur={() => void commit()}
          className={inputClass}
        />
      </Field>

      <Field
        label="Address"
        id={`${ids}-url`}
        hint={
          isUnencryptedRemote(provider.base_url)
            ? "Not encrypted: requests to this address can be read on the network."
            : local
              ? "Stays on this Mac."
              : `Requests go to ${hostOf(provider.base_url)}.`
        }
      >
        <input
          id={`${ids}-url`}
          value={draft.base_url}
          onChange={(e) => setDraft({ ...draft, base_url: e.target.value })}
          onBlur={() => void commit()}
          spellCheck={false}
          className={cn(inputClass, "font-mono text-[12.5px]")}
        />
      </Field>

      <ApiKeyField provider={provider} onError={setError} />

      <Field
        label="Model"
        id={`${ids}-model`}
        hint="Pick from the provider's list, or type a model ID."
      >
        <div className="flex gap-2">
          <input
            id={`${ids}-model`}
            list={listId}
            value={draft.model}
            onChange={(e) => setDraft({ ...draft, model: e.target.value })}
            onBlur={() => void commit()}
            placeholder="Model ID"
            spellCheck={false}
            className={cn(inputClass, "font-mono text-[12.5px]")}
          />
          <button
            type="button"
            onClick={() => void loadModels()}
            disabled={loading}
            className={cn(buttonClass, "shrink-0")}
          >
            {loading ? "Loading…" : "Load models"}
          </button>
        </div>
        {models && (
          <>
            <datalist id={listId}>
              {models.map((m) => (
                <option key={m.id} value={m.id}>
                  {[m.name, m.details].filter(Boolean).join(" · ")}
                </option>
              ))}
            </datalist>
            {models.length === 0 ? (
              <p className="mt-1 text-[12.5px] text-muted-foreground">
                The provider listed no models.
              </p>
            ) : (
              <select
                aria-label="Models from the provider"
                value=""
                onChange={(e) => e.target.value && void commit({ ...draft, model: e.target.value })}
                className={cn(inputClass, "mt-1.5")}
              >
                <option value="">{models.length} models found. Choose one…</option>
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name && m.name !== m.id ? `${m.name} (${m.id})` : m.id}
                    {m.details ? ` · ${m.details}` : ""}
                  </option>
                ))}
              </select>
            )}
          </>
        )}
        {provider.model && <ModelDetails provider={provider} />}
      </Field>

      <div>
        <p className="text-[13px] font-medium">Generation</p>
        <div className="mt-2 grid grid-cols-3 gap-3">
          {provider.kind === "anthropic" ? (
            <p className="text-[12px] leading-4 text-muted-foreground">
              Anthropic's current API has no temperature setting.
            </p>
          ) : (
            <NumberField
              key={`t-${draft.options.temperature}`}
              label="Temperature"
              value={draft.options.temperature}
              step="0.1"
              onCommit={(v) => setOption("temperature", v)}
            />
          )}
          <NumberField
            key={`m-${draft.options.max_output_tokens}`}
            label="Max output tokens"
            value={draft.options.max_output_tokens}
            placeholder={provider.kind === "anthropic" ? "4096" : "Default"}
            onCommit={(v) => setOption("max_output_tokens", v)}
          />
          <NumberField
            key={`c-${draft.options.context_window_override}`}
            label="Context window"
            value={draft.options.context_window_override}
            placeholder={provider.kind === "ollama" ? "8192" : "From model"}
            onCommit={(v) => setOption("context_window_override", v)}
          />
        </div>
      </div>

      {error && (
        <p role="alert" className="text-[13px] text-destructive">
          {error}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={() => void test()}
          disabled={report === "testing"}
          className={buttonClass}
        >
          {report === "testing" ? "Testing…" : "Test connection"}
        </button>
        <span role="status" className="min-w-0 flex-1 text-[13px]">
          {report && report !== "testing" && (
            <span className={report.ok ? "text-foreground" : "text-destructive"}>
              {report.message}
            </span>
          )}
        </span>
      </div>
      {!local && (
        <p className="-mt-2 text-[12px] text-muted-foreground">
          The test sends a one-line request ("Reply with the word OK.") to{" "}
          {hostOf(provider.base_url)}.
        </p>
      )}

      <div className="border-t pt-3">
        {confirmRemove ? (
          <div className="flex items-center gap-2 text-[13px]">
            <span>
              Remove {provider.name}
              {provider.has_credential ? " and delete its key from the Keychain" : ""}?
            </span>
            <button
              type="button"
              onClick={() => void save(ipc.aiRemoveProvider(provider.id), setError)}
              className="rounded-md px-2 py-1 text-destructive hover:bg-accent"
            >
              Remove
            </button>
            <button
              type="button"
              onClick={() => setConfirmRemove(false)}
              className="rounded-md px-2 py-1 text-muted-foreground hover:bg-accent"
            >
              Cancel
            </button>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setConfirmRemove(true)}
            className="rounded-md px-2 py-1 text-[13px] text-muted-foreground hover:bg-accent hover:text-destructive"
          >
            Remove provider
          </button>
        )}
      </div>
    </div>
  );
}

function ApiKeyField({
  provider,
  onError,
}: {
  provider: ProviderConfig;
  onError: (message: string | null) => void;
}) {
  const [key, setKey] = useState("");
  const [replacing, setReplacing] = useState(false);
  const id = useId();

  async function store() {
    // The key goes to the Keychain and is cleared from this form at once.
    const value = key;
    setKey("");
    if (await save(ipc.aiSetApiKey(provider.id, value), onError)) setReplacing(false);
  }

  const optional = !KEY_REQUIRED[provider.kind];
  if (provider.has_credential && !replacing) {
    return (
      <Field label="API key" id={id}>
        <div className="flex items-center gap-2 text-[13px]">
          <KeyRound className="size-3.5 text-muted-foreground" aria-hidden />
          <span id={id} className="flex-1">
            Stored in your Keychain.
          </span>
          <button type="button" onClick={() => setReplacing(true)} className={buttonClass}>
            Replace
          </button>
          <button
            type="button"
            onClick={() => void save(ipc.aiRemoveApiKey(provider.id), onError)}
            className={buttonClass}
          >
            Remove key
          </button>
        </div>
      </Field>
    );
  }
  return (
    <Field
      label="API key"
      id={id}
      hint={
        optional
          ? "Optional. Only if this server asks for one."
          : "Stored in your Keychain. Forma never shows it again."
      }
    >
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (key.trim()) void store();
        }}
      >
        <input
          id={id}
          type="password"
          autoComplete="off"
          spellCheck={false}
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder={provider.kind === "anthropic" ? "sk-ant-…" : "sk-…"}
          className={cn(inputClass, "font-mono")}
        />
        <button type="submit" disabled={!key.trim()} className={cn(buttonClass, "shrink-0")}>
          Save key
        </button>
        {replacing && (
          <button type="button" onClick={() => setReplacing(false)} className={buttonClass}>
            Cancel
          </button>
        )}
      </form>
    </Field>
  );
}

/** What the provider says about the chosen model, so the context window isn't a guess. */
function ModelDetails({ provider }: { provider: ProviderConfig }) {
  const [caps, setCaps] = useState<ModelCapabilities | null>(null);
  useEffect(() => {
    let current = true;
    setCaps(null);
    ipc
      .aiModelCapabilities(provider.id)
      .then((c) => current && setCaps(c))
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [provider.id]);

  if (!caps) return null;
  const parts: string[] = [];
  const window = caps.context_window;
  if (provider.kind === "ollama") {
    const used = provider.options.context_window_override ?? Math.min(window ?? 8192, 8192);
    parts.push(
      `Forma asks Ollama for a ${used.toLocaleString("en-US")}-token window${
        window ? ` (the model allows ${window.toLocaleString("en-US")})` : ""
      }.`,
    );
  } else if (window) {
    parts.push(`Context window: ${window.toLocaleString("en-US")} tokens.`);
  }
  if (caps.max_output_tokens) {
    parts.push(`Answers up to ${caps.max_output_tokens.toLocaleString("en-US")} tokens.`);
  }
  if (parts.length === 0) return null;
  return <p className="mt-1.5 text-[12.5px] text-muted-foreground">{parts.join(" ")}</p>;
}

function Field({
  label,
  id,
  hint,
  children,
}: {
  label: string;
  id: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div>
      <label htmlFor={id} className="text-[13px] font-medium">
        {label}
      </label>
      <div className="mt-1.5">{children}</div>
      {hint && <p className="mt-1 text-[12px] text-muted-foreground">{hint}</p>}
    </div>
  );
}

function NumberField({
  label,
  value,
  placeholder = "Default",
  step = "1",
  onCommit,
}: {
  label: string;
  value: number | null;
  placeholder?: string;
  step?: string;
  onCommit: (value: number | null) => void;
}) {
  const [text, setText] = useState(value === null ? "" : String(value));
  const id = useId();
  return (
    <div>
      <label htmlFor={id} className="text-[12px] text-muted-foreground">
        {label}
      </label>
      <input
        id={id}
        type="number"
        inputMode="decimal"
        step={step}
        min="0"
        value={text}
        placeholder={placeholder}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => {
          const parsed = text.trim() === "" ? null : Number(text);
          if (parsed !== null && !Number.isFinite(parsed)) return;
          if (parsed !== value) onCommit(parsed);
        }}
        className={cn(inputClass, "mt-1")}
      />
    </div>
  );
}
