//! Provider presets: default names and addresses, and whether a key is
//! needed (ARCHITECTURE §9.2). Also the rules for a provider's settings.

use std::net::IpAddr;

use forma_core::settings::{AiSettings, GenerationOptions, ProviderConfig, ProviderKind};
use serde::Deserialize;
use ts_rs::TS;
use url::Url;

pub struct Preset {
    pub name: &'static str,
    pub base_url: &'static str,
    /// Cloud APIs refuse every request without a key.
    pub key_required: bool,
}

pub fn preset(kind: ProviderKind) -> Preset {
    match kind {
        ProviderKind::Anthropic => Preset {
            name: "Anthropic",
            base_url: "https://api.anthropic.com",
            key_required: true,
        },
        ProviderKind::OpenAi => Preset {
            name: "OpenAI",
            base_url: "https://api.openai.com/v1",
            key_required: true,
        },
        ProviderKind::OpenAiCompatible => Preset {
            name: "OpenAI-compatible",
            base_url: "http://localhost:8000/v1",
            key_required: false,
        },
        ProviderKind::Ollama => Preset {
            name: "Ollama",
            base_url: "http://localhost:11434",
            key_required: false,
        },
        ProviderKind::LmStudio => Preset {
            name: "LM Studio",
            base_url: "http://localhost:1234/v1",
            key_required: false,
        },
    }
}

/// A new provider with the preset's defaults and no model yet. Its name is
/// made unique among `existing`.
pub fn new_provider(kind: ProviderKind, existing: &AiSettings) -> ProviderConfig {
    let base = preset(kind).name;
    let taken = |name: &str| existing.providers.iter().any(|p| p.name == name);
    let name = (1..)
        .map(|n| {
            if n == 1 {
                base.to_string()
            } else {
                format!("{base} {n}")
            }
        })
        .find(|name| !taken(name))
        .expect("an unused name exists");
    ProviderConfig {
        id: uuid::Uuid::new_v4().to_string(),
        kind,
        name,
        base_url: preset(kind).base_url.to_string(),
        model: String::new(),
        has_credential: false,
        options: GenerationOptions::default(),
    }
}

/// The fields of a provider the user edits in Settings → AI.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct ProviderUpdate {
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub options: GenerationOptions,
}

/// Checks `update` and applies it to `config`. Returns whether the address
/// changed host, in which case the user must agree again before messages
/// go there.
pub fn apply_update(config: &mut ProviderConfig, update: ProviderUpdate) -> Result<bool, String> {
    let name = update.name.trim();
    if name.is_empty() {
        return Err("Give the provider a name.".into());
    }
    if name.chars().count() > 60 {
        return Err("Use a name of 60 characters or fewer.".into());
    }
    let base_url = normalize_base_url(&update.base_url)?;
    let model = update.model.trim();
    if model.chars().any(char::is_control) {
        return Err("A model ID can't contain line breaks.".into());
    }
    let options = &update.options;
    if let Some(t) = options.temperature
        && !(0.0..=2.0).contains(&t)
    {
        return Err("Temperature must be between 0 and 2.".into());
    }
    if options
        .max_output_tokens
        .is_some_and(|n| !(1..=1_000_000).contains(&n))
    {
        return Err("Maximum output must be between 1 and 1,000,000 tokens.".into());
    }
    if options
        .context_window_override
        .is_some_and(|n| !(512..=10_000_000).contains(&n))
    {
        return Err("The context window must be between 512 and 10,000,000 tokens.".into());
    }
    let moved = destination(&config.base_url) != destination(&base_url);
    config.name = name.to_string();
    config.base_url = base_url;
    config.model = model.to_string();
    config.options = update.options;
    Ok(moved)
}

/// Scheme, host and port: where requests actually go.
fn destination(base_url: &str) -> Option<(String, String, Option<u16>)> {
    let url = Url::parse(base_url).ok()?;
    Some((
        url.scheme().to_string(),
        url.host_str()?.to_ascii_lowercase(),
        url.port_or_known_default(),
    ))
}

/// Checks and normalizes a base URL: `http` or `https`, a host, no query,
/// no credentials, no trailing slash.
pub fn normalize_base_url(input: &str) -> Result<String, String> {
    let url = Url::parse(input.trim())
        .map_err(|_| "Enter a full address, for example https://api.example.com/v1.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("The address must start with http:// or https://.".into());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("The address needs a host name.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Put the API key in the key field, not in the address.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("The address can't contain ? or #.".into());
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// Whether requests to `base_url` stay on this Mac (a loopback address).
pub fn is_local(base_url: &str) -> bool {
    let Ok(url) = Url::parse(base_url) else {
        return false;
    };
    match url.host() {
        Some(url::Host::Domain(d)) => {
            let d = d.to_ascii_lowercase();
            d == "localhost" || d.ends_with(".localhost")
        }
        Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
        Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
        None => false,
    }
}

/// The host and port, for messages and the "sent to" indicator.
pub fn host_of(base_url: &str) -> String {
    Url::parse(base_url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| u.port().map_or(h.to_string(), |p| format!("{h}:{p}")))
        })
        .unwrap_or_else(|| base_url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_urls_are_checked_and_normalized() {
        assert_eq!(
            normalize_base_url(" https://api.openai.com/v1/ ").unwrap(),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            normalize_base_url("http://localhost:11434").unwrap(),
            "http://localhost:11434"
        );
        for bad in [
            "api.openai.com",
            "ftp://x.com",
            "file:///etc/passwd",
            "https://user:pw@x.com",
            "https://x.com/v1?key=abc",
            "",
        ] {
            assert!(normalize_base_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_loopback_addresses_are_local() {
        assert!(is_local("http://localhost:11434"));
        assert!(is_local("http://127.0.0.1:1234/v1"));
        assert!(is_local("http://[::1]:8000"));
        assert!(is_local("http://ollama.localhost"));
        assert!(!is_local("http://192.168.1.20:11434"));
        assert!(!is_local("https://api.anthropic.com"));
        assert!(!is_local("http://localhost.example.com"));
        assert_eq!(host_of("http://localhost:11434/api"), "localhost:11434");
        assert_eq!(host_of("https://api.openai.com/v1"), "api.openai.com");
    }

    fn update(base_url: &str) -> ProviderUpdate {
        ProviderUpdate {
            name: " Work ".into(),
            base_url: base_url.into(),
            model: " gpt-6-astra ".into(),
            options: GenerationOptions::default(),
        }
    }

    #[test]
    fn new_providers_get_unique_names_and_preset_addresses() {
        let mut ai = AiSettings::default();
        let first = new_provider(ProviderKind::Ollama, &ai);
        assert_eq!(first.base_url, "http://localhost:11434");
        assert!(first.model.is_empty() && !first.has_credential);
        ai.providers.push(first);
        assert_eq!(new_provider(ProviderKind::Ollama, &ai).name, "Ollama 2");
    }

    #[test]
    fn updates_are_checked_and_report_a_new_destination() {
        let mut config = new_provider(ProviderKind::OpenAi, &AiSettings::default());
        assert!(!apply_update(&mut config, update("https://api.openai.com/v1/")).unwrap());
        assert_eq!(config.name, "Work");
        assert_eq!(config.model, "gpt-6-astra");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert!(apply_update(&mut config, update("https://proxy.example.com/v1")).unwrap());

        let before = config.clone();
        let mut bad = update("https://api.openai.com/v1");
        bad.options.temperature = Some(3.0);
        assert!(apply_update(&mut config, bad).is_err());
        let mut bad = update("https://api.openai.com/v1");
        bad.name = "  ".into();
        assert!(apply_update(&mut config, bad).is_err());
        assert!(apply_update(&mut config, update("not a url")).is_err());
        assert_eq!(config, before, "a refused update changes nothing");
    }
}
