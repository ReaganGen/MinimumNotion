//! Talks to the real Anthropic and OpenAI APIs with a key that can't work,
//! to check TLS, the auth headers and how real rejections are reported.
//! Needs the internet, so it's ignored by default:
//!
//!   cargo test -p forma-ai --test live_endpoints -- --ignored

use forma_ai::AiErrorKind;
use forma_ai::provider::{Provider, http_client};
use forma_core::settings::{GenerationOptions, ProviderConfig, ProviderKind};

const FAKE_KEY: &str = "sk-forma-invalid-test-key";

fn config(kind: ProviderKind, base_url: &str) -> ProviderConfig {
    ProviderConfig {
        id: "live".into(),
        kind,
        name: forma_ai::presets::preset(kind).name.into(),
        base_url: base_url.into(),
        model: String::new(),
        has_credential: true,
        options: GenerationOptions::default(),
    }
}

#[tokio::test]
#[ignore = "needs the internet"]
async fn real_apis_reject_a_wrong_key_as_an_auth_error() {
    let client = http_client().unwrap();
    for (kind, url) in [
        (ProviderKind::Anthropic, "https://api.anthropic.com"),
        (ProviderKind::OpenAi, "https://api.openai.com/v1"),
    ] {
        let config = config(kind, url);
        let err = Provider::new(&config, Some(FAKE_KEY), &client)
            .list_models()
            .await
            .unwrap_err()
            .redacted(Some(FAKE_KEY));
        assert_eq!(err.kind, AiErrorKind::Auth, "{kind:?}: {err:?}");
        assert_eq!(err.status, Some(401));
        assert!(!err.message.contains(FAKE_KEY), "{}", err.message);
        println!("{kind:?}: {}", err.message);
    }
}
