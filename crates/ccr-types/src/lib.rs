use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(rename = "APIKEY")]
    pub api_key: Option<String>,
    #[serde(rename = "HOST")]
    pub host: Option<String>,
    #[serde(rename = "PORT")]
    pub port: Option<u16>,
    #[serde(rename = "Providers")]
    pub providers: Vec<Provider>,
    #[serde(rename = "Router")]
    pub router: RouterConfig,
    #[serde(rename = "Proxy", skip_serializing_if = "Option::is_none")]
    pub proxy: Option<ProxyConfig>,
    #[serde(rename = "RoutePool", default, skip_serializing_if = "Option::is_none")]
    pub route_pool: Option<RoutePoolConfig>,
    #[serde(rename = "AppSettings", default)]
    pub app_settings: AppSettings,
}

impl Config {
    pub fn first_route_pool_route(&self) -> Option<&str> {
        let mut candidates = self
            .route_pool
            .as_ref()?
            .candidates
            .iter()
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.route.cmp(&b.route))
        });
        candidates
            .into_iter()
            .find(|candidate| candidate.enabled && !candidate.route.trim().is_empty())
            .map(|candidate| candidate.route.trim())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_kind: Option<ProviderApiKind>,
    #[serde(default)]
    pub api_kind_source: ProviderApiKindSource,
    pub api_base_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoint_candidates: Vec<String>,
    #[serde(default)]
    pub transformer: TransformerConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderApiKind {
    #[serde(rename = "openai_chat")]
    OpenAiChat,
    #[serde(rename = "openai_responses")]
    OpenAiResponses,
    AnthropicMessages,
    AnthropicCompatible,
    OpenRouter,
    DeepSeek,
    Groq,
    Vercel,
    Custom,
}

impl ProviderApiKind {
    pub const ALL: [ProviderApiKind; 9] = [
        ProviderApiKind::OpenAiChat,
        ProviderApiKind::OpenAiResponses,
        ProviderApiKind::AnthropicMessages,
        ProviderApiKind::AnthropicCompatible,
        ProviderApiKind::OpenRouter,
        ProviderApiKind::DeepSeek,
        ProviderApiKind::Groq,
        ProviderApiKind::Vercel,
        ProviderApiKind::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ProviderApiKind::OpenAiChat => "OpenAI Chat Completions",
            ProviderApiKind::OpenAiResponses => "OpenAI Responses",
            ProviderApiKind::AnthropicMessages => "Anthropic Messages",
            ProviderApiKind::AnthropicCompatible => "Anthropic-compatible for Claude Code",
            ProviderApiKind::OpenRouter => "OpenRouter",
            ProviderApiKind::DeepSeek => "DeepSeek",
            ProviderApiKind::Groq => "Groq",
            ProviderApiKind::Vercel => "Vercel",
            ProviderApiKind::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderApiKindSource {
    Explicit,
    #[default]
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransformerConfig {
    #[serde(rename = "use", default)]
    pub use_transformers: Vec<serde_json::Value>,
    #[serde(flatten)]
    pub model_overrides: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouterConfig {
    /// Legacy configuration field retained for deserializing existing configs.
    /// Server runtime does not use this for upstream selection; Route Pool
    /// candidates are the only runtime routing source.
    pub background: Option<String>,
    /// Legacy configuration field retained for deserializing existing configs.
    /// Server runtime does not use this for upstream selection.
    pub think: Option<String>,
    /// Legacy configuration field retained for deserializing existing configs.
    /// Server runtime does not use this for upstream selection.
    #[serde(rename = "longContext")]
    pub long_context: Option<String>,
    /// Legacy tokenizer threshold paired with `longContext`; not runtime routing policy.
    #[serde(rename = "longContextThreshold")]
    pub long_context_threshold: Option<u64>,
    /// Legacy configuration field retained for deserializing existing configs.
    /// Server runtime does not use this for upstream selection.
    #[serde(rename = "webSearch")]
    pub web_search: Option<String>,
    /// Legacy configuration field retained for deserializing existing configs.
    /// Server runtime does not use this for upstream selection.
    pub image: Option<String>,
    #[serde(rename = "tokenizerBackend", default)]
    pub tokenizer_backend: TokenizerBackend,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub https: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socks5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_proxy: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutePoolConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(
        rename = "failureThreshold",
        default = "default_route_pool_failure_threshold"
    )]
    pub failure_threshold: u32,
    #[serde(rename = "banSeconds", default = "default_route_pool_ban_seconds")]
    pub ban_seconds: u64,
    #[serde(default)]
    pub candidates: Vec<RoutePoolCandidate>,
}

impl Default for RoutePoolConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            failure_threshold: default_route_pool_failure_threshold(),
            ban_seconds: default_route_pool_ban_seconds(),
            candidates: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePoolCandidate {
    pub route: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub auto_launch: bool,
    #[serde(default = "default_true")]
    pub server_auto_start: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub claude_config_path: Option<String>,
    #[serde(default)]
    pub codex_config_path: Option<String>,
    #[serde(default)]
    pub opencode_config_path: Option<String>,
    #[serde(default)]
    pub openclaw_config_path: Option<String>,
    #[serde(default)]
    pub hermes_config_path: Option<String>,
    #[serde(default)]
    pub global_proxy: Option<String>,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub claude_code_models: ClaudeCodeModelSettings,
    #[serde(default)]
    pub claude_code_models_enabled: bool,
    #[serde(default)]
    pub admin_api_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            auto_launch: false,
            server_auto_start: true,
            log_level: default_log_level(),
            claude_config_path: None,
            codex_config_path: None,
            opencode_config_path: None,
            openclaw_config_path: None,
            hermes_config_path: None,
            global_proxy: None,
            theme: String::new(),
            claude_code_models: ClaudeCodeModelSettings::default(),
            claude_code_models_enabled: false,
            admin_api_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeCodeModelSettings {
    #[serde(default = "default_claude_code_model")]
    pub model: String,
    #[serde(default = "default_claude_code_haiku_model")]
    pub haiku_model: String,
    #[serde(default = "default_claude_code_sonnet_model")]
    pub sonnet_model: String,
    #[serde(default = "default_claude_code_opus_model")]
    pub opus_model: String,
}

impl Default for ClaudeCodeModelSettings {
    fn default() -> Self {
        Self {
            model: default_claude_code_model(),
            haiku_model: default_claude_code_haiku_model(),
            sonnet_model: default_claude_code_sonnet_model(),
            opus_model: default_claude_code_opus_model(),
        }
    }
}

impl ClaudeCodeModelSettings {
    pub fn is_empty(&self) -> bool {
        self.model.trim().is_empty()
            && self.haiku_model.trim().is_empty()
            && self.sonnet_model.trim().is_empty()
            && self.opus_model.trim().is_empty()
    }
}

fn default_true() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_route_pool_failure_threshold() -> u32 {
    3
}

fn default_route_pool_ban_seconds() -> u64 {
    3600
}

fn default_claude_code_model() -> String {
    String::new()
}

fn default_claude_code_haiku_model() -> String {
    String::new()
}

fn default_claude_code_sonnet_model() -> String {
    String::new()
}

fn default_claude_code_opus_model() -> String {
    String::new()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase", untagged)]
pub enum TokenizerBackend {
    #[default]
    Tiktoken,
    Huggingface,
    Api {
        endpoint: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub system: Option<serde_json::Value>,
    pub tools: Option<Vec<serde_json::Value>>,
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: bool,
    pub thinking: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let c = Config::default();
        assert!(c.api_key.is_none());
        assert!(c.providers.is_empty());
        assert!(c.app_settings.server_auto_start);
    }

    #[test]
    fn config_deserialize() {
        let json = r#"{"Providers":[],"Router":{},"RoutePool":{"enabled":true,"candidates":[{"route":"openai,gpt-4o"}]}}"#;
        let c: Config = serde_json::from_str(json).unwrap();
        assert_eq!(c.first_route_pool_route(), Some("openai,gpt-4o"));
        assert!(c.app_settings.server_auto_start);
        assert_eq!(c.app_settings.claude_code_models.model, "");
        assert_eq!(c.app_settings.claude_code_models.haiku_model, "");
        assert_eq!(c.app_settings.hermes_config_path, None);
    }

    #[test]
    fn claude_code_model_settings_deserializes_custom_mapping() {
        let json = r#"{
  "Providers": [],
  "Router": {},
  "AppSettings": {
    "claude_code_models": {
      "model": "main",
      "haiku_model": "haiku",
      "sonnet_model": "sonnet",
      "opus_model": "opus"
    }
  }
}"#;
        let c: Config = serde_json::from_str(json).unwrap();
        assert_eq!(c.app_settings.claude_code_models.model, "main");
        assert_eq!(c.app_settings.claude_code_models.haiku_model, "haiku");
        assert_eq!(c.app_settings.claude_code_models.sonnet_model, "sonnet");
        assert_eq!(c.app_settings.claude_code_models.opus_model, "opus");
    }

    #[test]
    fn claude_code_model_settings_detects_empty_mapping() {
        assert!(ClaudeCodeModelSettings::default().is_empty());
        assert!(
            !ClaudeCodeModelSettings {
                model: "main".to_string(),
                ..Default::default()
            }
            .is_empty()
        );
    }

    #[test]
    fn provider_endpoint_candidates_default_empty() {
        let json = r#"{"name":"p","api_base_url":"https://x","api_key":"k","models":[]}"#;
        let p: Provider = serde_json::from_str(json).unwrap();
        assert!(p.endpoint_candidates.is_empty());
        assert_eq!(p.api_kind, None);
        assert_eq!(p.api_kind_source, ProviderApiKindSource::Inferred);
    }

    #[test]
    fn provider_api_kind_deserializes_explicit_choice() {
        let json = r#"{"name":"p","api_kind":"openai_responses","api_kind_source":"explicit","api_base_url":"https://x","api_key":"k","models":[]}"#;
        let p: Provider = serde_json::from_str(json).unwrap();
        assert_eq!(p.api_kind, Some(ProviderApiKind::OpenAiResponses));
        assert_eq!(p.api_kind_source, ProviderApiKindSource::Explicit);
    }

    #[test]
    fn route_pool_defaults_are_stable() {
        let config = RoutePoolConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.ban_seconds, 3600);
        assert!(config.candidates.is_empty());
    }

    #[test]
    fn route_pool_deserializes_route_pool_name() {
        let json = r#"{"Providers":[],"Router":{},"RoutePool":{"enabled":true,"candidates":[{"route":"p"}]}}"#;
        let config: Config = serde_json::from_str(json).unwrap();

        let pool = config.route_pool.unwrap();
        assert!(pool.enabled);
        assert_eq!(pool.candidates[0].route, "p");
    }

    #[test]
    fn first_route_pool_route_uses_enabled_priority_order() {
        let config: Config = serde_json::from_str(
            r#"{"Providers":[],"Router":{},"RoutePool":{"enabled":true,"candidates":[
                {"route":"b","enabled":true,"priority":2},
                {"route":"a","enabled":false,"priority":1},
                {"route":"c","enabled":true,"priority":1}
            ]}}"#,
        )
        .unwrap();

        assert_eq!(config.first_route_pool_route(), Some("c"));
    }

    #[test]
    fn route_pool_candidate_defaults_enabled() {
        let json = r#"{"route":"zenmux"}"#;
        let candidate: RoutePoolCandidate = serde_json::from_str(json).unwrap();

        assert!(candidate.enabled);
        assert_eq!(candidate.priority, 0);
    }

    #[test]
    fn tokenizer_backend_default_is_tiktoken() {
        assert_eq!(TokenizerBackend::default(), TokenizerBackend::Tiktoken);
    }

    #[test]
    fn messages_request_stream_default_false() {
        let json = r#"{"model":"claude-3-5-sonnet","messages":[],"max_tokens":100}"#;
        let r: MessagesRequest = serde_json::from_str(json).unwrap();
        assert!(!r.stream);
    }
}
