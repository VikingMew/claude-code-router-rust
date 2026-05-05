use ccr_types::{Config, Provider, ProviderApiKind, ProviderApiKindSource, TransformerConfig};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointTestMode {
    OpenAiChat,
    OpenAiResponses,
    AnthropicMessages,
    BasicPost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientInjectionMode {
    ClaudeDirect,
    CodexResponsesDirect,
    InRouterOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderApiKindChoice {
    pub kind: ProviderApiKind,
    pub source: ProviderApiKindSource,
    pub inferred_kind: ProviderApiKind,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderApiKindDefaults {
    pub default_endpoint: Option<&'static str>,
    pub recommended_transformers: Vec<&'static str>,
    pub endpoint_test_mode: EndpointTestMode,
    pub client_injection_mode: ClientInjectionMode,
}

pub fn resolve_provider_api_kind(provider: &Provider) -> ProviderApiKindChoice {
    let inferred_kind = infer_provider_api_kind(provider);
    match provider.api_kind {
        Some(kind) => {
            let warning = (provider.api_kind_source == ProviderApiKindSource::Explicit
                && kind != inferred_kind
                && inferred_kind != ProviderApiKind::Custom)
                .then(|| {
                    format!(
                        "Explicit API kind '{}' differs from inferred '{}'.",
                        kind.label(),
                        inferred_kind.label()
                    )
                });
            ProviderApiKindChoice {
                kind,
                source: provider.api_kind_source,
                inferred_kind,
                warning,
            }
        }
        None => ProviderApiKindChoice {
            kind: inferred_kind,
            source: ProviderApiKindSource::Inferred,
            inferred_kind,
            warning: None,
        },
    }
}

pub fn mark_provider_api_kind_explicit(provider: &mut Provider, kind: ProviderApiKind) {
    provider.api_kind = Some(kind);
    provider.api_kind_source = ProviderApiKindSource::Explicit;
}

pub fn mark_provider_api_kind_inferred(provider: &mut Provider) {
    provider.api_kind = Some(infer_provider_api_kind(provider));
    provider.api_kind_source = ProviderApiKindSource::Inferred;
}

pub fn ensure_inferred_provider_api_kinds(config: &mut Config) {
    for provider in &mut config.providers {
        if provider.api_kind.is_none() {
            mark_provider_api_kind_inferred(provider);
        }
    }
}

pub fn infer_provider_api_kind(provider: &Provider) -> ProviderApiKind {
    let transformer_names = transformer_names(&provider.transformer);
    if transformer_names.iter().any(|name| name == "openrouter") {
        return ProviderApiKind::OpenRouter;
    }
    if transformer_names.iter().any(|name| name == "deepseek") {
        return ProviderApiKind::DeepSeek;
    }
    if transformer_names.iter().any(|name| name == "groq") {
        return ProviderApiKind::Groq;
    }
    if transformer_names.iter().any(|name| name == "vercel") {
        return ProviderApiKind::Vercel;
    }
    if transformer_names.iter().any(|name| name == "anthropic") {
        return ProviderApiKind::AnthropicMessages;
    }
    if transformer_names.iter().any(|name| name == "openai") {
        return if provider.api_base_url.contains("/responses") {
            ProviderApiKind::OpenAiResponses
        } else {
            ProviderApiKind::OpenAiChat
        };
    }

    let name = provider.name.to_ascii_lowercase();
    let url = provider.api_base_url.to_ascii_lowercase();
    if name.contains("openrouter") || url.contains("openrouter.ai") {
        ProviderApiKind::OpenRouter
    } else if name.contains("deepseek") || url.contains("deepseek") {
        ProviderApiKind::DeepSeek
    } else if name.contains("groq") || url.contains("groq.com") {
        ProviderApiKind::Groq
    } else if name.contains("vercel") || url.contains("vercel") {
        ProviderApiKind::Vercel
    } else if url.contains("/responses") {
        ProviderApiKind::OpenAiResponses
    } else if url.contains("/chat/completions") {
        ProviderApiKind::OpenAiChat
    } else if url.contains("/v1/messages") || url.contains("/anthropic") {
        ProviderApiKind::AnthropicMessages
    } else if name.contains("anthropic") || name.contains("claude") {
        ProviderApiKind::AnthropicMessages
    } else {
        ProviderApiKind::Custom
    }
}

pub fn provider_api_kind_defaults(
    kind: ProviderApiKind,
    model: Option<&str>,
) -> ProviderApiKindDefaults {
    let mut recommended_transformers = match kind {
        ProviderApiKind::OpenAiChat | ProviderApiKind::OpenAiResponses => vec!["openai"],
        ProviderApiKind::AnthropicMessages | ProviderApiKind::AnthropicCompatible => {
            vec!["anthropic"]
        }
        ProviderApiKind::OpenRouter => vec!["openrouter"],
        ProviderApiKind::DeepSeek => vec!["deepseek"],
        ProviderApiKind::Groq => vec!["groq"],
        ProviderApiKind::Vercel => vec!["vercel"],
        ProviderApiKind::Custom => Vec::new(),
    };

    if kind == ProviderApiKind::DeepSeek
        && model
            .map(|model| {
                let model = model.to_ascii_lowercase();
                model.contains("chat") && !model.contains("reasoner")
            })
            .unwrap_or(false)
    {
        recommended_transformers.push("tooluse");
    }

    ProviderApiKindDefaults {
        default_endpoint: match kind {
            ProviderApiKind::OpenAiChat => Some("https://api.openai.com/v1/chat/completions"),
            ProviderApiKind::OpenAiResponses => Some("https://api.openai.com/v1/responses"),
            ProviderApiKind::AnthropicMessages => Some("https://api.anthropic.com/v1/messages"),
            ProviderApiKind::AnthropicCompatible => None,
            ProviderApiKind::OpenRouter => Some("https://openrouter.ai/api/v1/chat/completions"),
            ProviderApiKind::DeepSeek => Some("https://api.deepseek.com/chat/completions"),
            ProviderApiKind::Groq => Some("https://api.groq.com/openai/v1/chat/completions"),
            ProviderApiKind::Vercel => Some("https://ai-gateway.vercel.sh/v1/chat/completions"),
            ProviderApiKind::Custom => None,
        },
        recommended_transformers,
        endpoint_test_mode: match kind {
            ProviderApiKind::OpenAiChat
            | ProviderApiKind::OpenRouter
            | ProviderApiKind::DeepSeek
            | ProviderApiKind::Groq
            | ProviderApiKind::Vercel => EndpointTestMode::OpenAiChat,
            ProviderApiKind::OpenAiResponses => EndpointTestMode::OpenAiResponses,
            ProviderApiKind::AnthropicMessages | ProviderApiKind::AnthropicCompatible => {
                EndpointTestMode::AnthropicMessages
            }
            ProviderApiKind::Custom => EndpointTestMode::BasicPost,
        },
        client_injection_mode: match kind {
            ProviderApiKind::AnthropicMessages | ProviderApiKind::AnthropicCompatible => {
                ClientInjectionMode::ClaudeDirect
            }
            ProviderApiKind::OpenAiResponses => ClientInjectionMode::CodexResponsesDirect,
            _ => ClientInjectionMode::InRouterOnly,
        },
    }
}

pub fn transformer_names(transformer: &TransformerConfig) -> Vec<String> {
    transformer
        .use_transformers
        .iter()
        .filter_map(transformer_name_from_value)
        .collect()
}

fn transformer_name_from_value(value: &Value) -> Option<String> {
    if let Some(name) = value.as_str() {
        return Some(name.to_ascii_lowercase());
    }
    value
        .as_array()
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::TransformerConfig;

    fn provider(name: &str, url: &str) -> Provider {
        Provider {
            name: name.to_string(),
            api_kind: None,
            api_kind_source: ProviderApiKindSource::Inferred,
            api_base_url: url.to_string(),
            api_key: "key".to_string(),
            models: vec!["model".to_string()],
            endpoint_candidates: vec![],
            transformer: TransformerConfig::default(),
        }
    }

    #[test]
    fn infers_openai_responses_from_url() {
        assert_eq!(
            infer_provider_api_kind(&provider("openai", "https://api.openai.com/v1/responses")),
            ProviderApiKind::OpenAiResponses
        );
    }

    #[test]
    fn infers_openai_chat_from_url() {
        assert_eq!(
            infer_provider_api_kind(&provider(
                "openai",
                "https://api.openai.com/v1/chat/completions"
            )),
            ProviderApiKind::OpenAiChat
        );
    }

    #[test]
    fn infers_anthropic_messages_from_url() {
        assert_eq!(
            infer_provider_api_kind(&provider(
                "anthropic",
                "https://api.anthropic.com/v1/messages"
            )),
            ProviderApiKind::AnthropicMessages
        );
    }

    #[test]
    fn transformer_overrides_url_inference() {
        let mut provider = provider("custom", "https://example.com/v1/chat/completions");
        provider.transformer.use_transformers = vec![serde_json::json!("openrouter")];

        assert_eq!(
            infer_provider_api_kind(&provider),
            ProviderApiKind::OpenRouter
        );
    }

    #[test]
    fn explicit_choice_wins_over_inference_and_warns() {
        let mut provider = provider("openai", "https://api.openai.com/v1/responses");
        provider.api_kind = Some(ProviderApiKind::AnthropicMessages);
        provider.api_kind_source = ProviderApiKindSource::Explicit;

        let choice = resolve_provider_api_kind(&provider);

        assert_eq!(choice.kind, ProviderApiKind::AnthropicMessages);
        assert_eq!(choice.inferred_kind, ProviderApiKind::OpenAiResponses);
        assert!(choice.warning.is_some());
    }

    #[test]
    fn inferred_choice_has_no_warning() {
        let provider = provider("openai", "https://api.openai.com/v1/responses");

        let choice = resolve_provider_api_kind(&provider);

        assert_eq!(choice.kind, ProviderApiKind::OpenAiResponses);
        assert_eq!(choice.source, ProviderApiKindSource::Inferred);
        assert!(choice.warning.is_none());
    }

    #[test]
    fn mark_explicit_persists_user_choice() {
        let mut provider = provider("openai", "https://api.openai.com/v1/responses");

        mark_provider_api_kind_explicit(&mut provider, ProviderApiKind::OpenAiChat);

        assert_eq!(provider.api_kind, Some(ProviderApiKind::OpenAiChat));
        assert_eq!(provider.api_kind_source, ProviderApiKindSource::Explicit);
    }

    #[test]
    fn mark_inferred_uses_latest_provider_fields() {
        let mut provider = provider("openai", "https://api.openai.com/v1/responses");
        provider.api_base_url = "https://api.openai.com/v1/chat/completions".to_string();

        mark_provider_api_kind_inferred(&mut provider);

        assert_eq!(provider.api_kind, Some(ProviderApiKind::OpenAiChat));
        assert_eq!(provider.api_kind_source, ProviderApiKindSource::Inferred);
    }

    #[test]
    fn ensure_inferred_provider_api_kinds_does_not_override_explicit_choice() {
        let mut config = Config {
            providers: vec![provider("openai", "https://api.openai.com/v1/responses"), {
                let mut provider = provider("anthropic", "https://api.anthropic.com/v1/messages");
                provider.api_kind = Some(ProviderApiKind::OpenAiChat);
                provider.api_kind_source = ProviderApiKindSource::Explicit;
                provider
            }],
            ..Default::default()
        };

        ensure_inferred_provider_api_kinds(&mut config);

        assert_eq!(
            config.providers[0].api_kind,
            Some(ProviderApiKind::OpenAiResponses)
        );
        assert_eq!(
            config.providers[0].api_kind_source,
            ProviderApiKindSource::Inferred
        );
        assert_eq!(
            config.providers[1].api_kind,
            Some(ProviderApiKind::OpenAiChat)
        );
        assert_eq!(
            config.providers[1].api_kind_source,
            ProviderApiKindSource::Explicit
        );
    }

    #[test]
    fn deepseek_chat_recommends_tooluse() {
        let defaults = provider_api_kind_defaults(ProviderApiKind::DeepSeek, Some("deepseek-chat"));

        assert_eq!(
            defaults.recommended_transformers,
            vec!["deepseek", "tooluse"]
        );
    }

    #[test]
    fn anthropic_does_not_recommend_tooluse() {
        let defaults =
            provider_api_kind_defaults(ProviderApiKind::AnthropicMessages, Some("claude-sonnet-4"));

        assert_eq!(defaults.recommended_transformers, vec!["anthropic"]);
    }

    #[test]
    fn codex_direct_is_limited_to_openai_responses() {
        assert_eq!(
            provider_api_kind_defaults(ProviderApiKind::OpenAiResponses, None)
                .client_injection_mode,
            ClientInjectionMode::CodexResponsesDirect
        );
        assert_eq!(
            provider_api_kind_defaults(ProviderApiKind::OpenAiChat, None).client_injection_mode,
            ClientInjectionMode::InRouterOnly
        );
    }
}
