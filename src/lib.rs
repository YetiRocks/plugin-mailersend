#![warn(missing_docs)]
//! MailerSend transactional email provider for Yeti.
//!
//! Registers a factory under the name `"mailersend"` so
//! `yeti_sdk::mail::instantiate_provider("mailersend", &cfg)` can
//! construct the provider from yeti-config.yaml's `email.provider`
//! subtree.
//!
//! ## Config shape
//!
//! ```yaml
//! email:
//!   provider:
//!     name: mailersend
//!     apiToken: "${MAILERSEND_TOKEN}"
//!     from: "no-reply@yeti.run"
//!     fromName: "Yeti"
//!     # endpoint is optional; defaults to api.mailersend.com/v1/email
//!     endpoint: https://api.mailersend.com/v1/email
//! ```
//!
//! ## Why not an official SDK
//!
//! MailerSend publishes SDKs for PHP, Node, Python, Ruby, and Go but
//! not Rust as of 2026-04. Per AGENTS.md "prefer established crates"
//! rule, criterion #2 ("works out of the box") fails — so this
//! ~60-line reqwest client is the right call. Swap-in the first time
//! an official Rust SDK ships.
//!
//! ## Registering
//!
//! yeti loads this crate as a static plugin. On startup the
//! plugin calls `yeti_sdk::mail::register_provider_factory` so
//! later config resolution finds it by name.

use std::sync::Arc;

use serde::Deserialize;
use yeti_sdk::error::{Result, YetiError};
use yeti_sdk::mail::{Email, EmailProvider, register_provider_factory};

/// Config shape deserialized from `email.provider.*`. The plugin
/// controls this schema; yeti-sdk passes the raw JSON subtree.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct MailerSendConfigYaml {
    /// Transactional API token from MailerSend dashboard → Domains →
    /// API tokens.
    api_token: String,
    /// RFC 5322 From address. Must be on a verified domain in your
    /// MailerSend account.
    from: String,
    /// Optional display name paired with `from`.
    from_name: Option<String>,
    /// Override the API endpoint. Defaults to
    /// `https://api.mailersend.com/v1/email` when empty.
    endpoint: Option<String>,
}

const DEFAULT_ENDPOINT: &str = "https://api.mailersend.com/v1/email";

/// Transactional email provider backed by MailerSend.
pub struct MailerSendProvider {
    api_token: String,
    from: String,
    from_name: Option<String>,
    endpoint: String,
    client: reqwest::Client,
}

impl MailerSendProvider {
    /// Build a provider from the deserialized config.
    fn new(cfg: MailerSendConfigYaml) -> Result<Self> {
        if cfg.api_token.is_empty() {
            return Err(YetiError::Validation(
                "mailersend: apiToken is required".into(),
            ));
        }
        if cfg.from.is_empty() {
            return Err(YetiError::Validation("mailersend: from is required".into()));
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| YetiError::Internal(format!("build mailersend client: {e}")))?;
        let endpoint = cfg
            .endpoint
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
        Ok(Self {
            api_token: cfg.api_token,
            from: cfg.from,
            from_name: cfg.from_name,
            endpoint,
            client,
        })
    }
}

#[async_trait::async_trait]
impl EmailProvider for MailerSendProvider {
    async fn send(&self, email: &Email) -> Result<()> {
        let mut from = serde_json::json!({"email": self.from});
        if let Some(name) = &self.from_name {
            from["name"] = serde_json::Value::String(name.clone());
        }

        let mut payload = serde_json::json!({
            "from": from,
            "to": [{"email": email.to}],
            "subject": email.subject,
            "text": email.body_text,
        });
        if let Some(html) = &email.body_html {
            payload["html"] = serde_json::Value::String(html.clone());
        }

        let resp = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_token)
            .header("Accept", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| YetiError::Internal(format!("mailersend send: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body_snippet = resp
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(500)
                .collect::<String>();
            return Err(YetiError::Internal(format!(
                "mailersend send returned {status}: {body_snippet}"
            )));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "mailersend"
    }
}

/// Register the MailerSend factory under the name `"mailersend"`.
/// Called once at startup — yeti-core invokes this from the static
/// plugin registration path.
pub fn register() {
    register_provider_factory(
        "mailersend",
        Box::new(|cfg| {
            let typed: MailerSendConfigYaml = serde_json::from_value(cfg.clone())
                .map_err(|e| YetiError::Validation(format!("mailersend config: {e}")))?;
            let provider = MailerSendProvider::new(typed)?;
            Ok(Arc::new(provider) as Arc<dyn EmailProvider>)
        }),
    );
}

// ============================================================================
// Plugin hook — slots into the yeti-runtime static plugin registry
// ============================================================================

use yeti_sdk::plugins::{RegistrationContext, Plugin, StartupContext};
use yeti_sdk::resource::Context;

/// Create the plugin-mailersend instance.
pub fn plugin() -> Box<dyn Plugin> {
    Box::new(MailerSendPlugin)
}

struct MailerSendPlugin;

impl Plugin for MailerSendPlugin {
    fn id(&self) -> &'static str {
        "plugin-mailersend"
    }

    fn name(&self) -> &'static str {
        "MailerSend Email Provider"
    }

    fn config_toml(&self) -> Option<&'static str> {
        Some(include_str!("../Cargo.toml"))
    }

    fn depends_on(&self) -> &[&'static str] {
        &[]
    }

    fn is_plugin(&self) -> bool {
        true
    }

    fn is_required(&self, _ctx: &StartupContext) -> bool {
        // Only load when the operator has actually configured
        // mailersend as the active provider. Otherwise this crate
        // sits idle in the binary and contributes nothing.
        yeti_sdk::plugins::plugins_config()
            .email
            .provider
            .as_ref()
            .map(|p| p.name == "mailersend")
            .unwrap_or(false)
    }

    fn schemas(&self) -> Vec<&'static str> {
        Vec::new()
    }

    fn resources(&self, _ctx: &mut RegistrationContext) -> yeti_sdk::error::Result<()> {
        register();
        Ok(())
    }

    fn on_ready(&self, _ctx: &Context) -> yeti_sdk::error::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_rejects_missing_api_token() {
        let cfg = MailerSendConfigYaml {
            api_token: String::new(),
            from: "from@example.com".into(),
            from_name: None,
            endpoint: None,
        };
        assert!(MailerSendProvider::new(cfg).is_err());
    }

    #[test]
    fn config_rejects_missing_from() {
        let cfg = MailerSendConfigYaml {
            api_token: "secret".into(),
            from: String::new(),
            from_name: None,
            endpoint: None,
        };
        assert!(MailerSendProvider::new(cfg).is_err());
    }

    /// reqwest::Client::builder() panics without a TLS provider — we
    /// install rustls/ring here for every test that needs a real
    /// client instance.
    fn ensure_tls() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    #[test]
    fn config_defaults_endpoint_when_empty() {
        ensure_tls();
        let cfg = MailerSendConfigYaml {
            api_token: "secret".into(),
            from: "from@example.com".into(),
            from_name: None,
            endpoint: Some(String::new()),
        };
        let p = MailerSendProvider::new(cfg).unwrap();
        assert_eq!(p.endpoint, DEFAULT_ENDPOINT);
    }

    #[test]
    fn factory_registration_roundtrips_via_json() {
        ensure_tls();
        register();
        let cfg = serde_json::json!({
            "apiToken": "secret",
            "from": "from@example.com",
            "fromName": "Yeti",
        });
        let p = yeti_sdk::mail::instantiate_provider("mailersend", &cfg).unwrap();
        assert_eq!(p.name(), "mailersend");
    }
}
