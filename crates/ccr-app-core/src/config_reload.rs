use ccr_types::Config;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigReloadResult {
    Success(String),
    AdminApiUnavailable(String),
    Unauthorized(String),
    RequestFailed(String),
    ReloadFailed(String),
}

impl ConfigReloadResult {
    pub fn ui_message(&self) -> String {
        match self {
            ConfigReloadResult::Success(message) => format!("✓ {message}"),
            ConfigReloadResult::AdminApiUnavailable(message)
            | ConfigReloadResult::Unauthorized(message)
            | ConfigReloadResult::RequestFailed(message)
            | ConfigReloadResult::ReloadFailed(message) => format!("✗ {message}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ReloadResponse {
    success: bool,
    message: String,
}

pub fn admin_reload_enabled(config: &Config) -> bool {
    config.app_settings.admin_api_enabled
}

pub fn admin_reload_unavailable_message() -> &'static str {
    "Running server reload requires Admin API to be enabled in config."
}

pub fn reload_running_server_via_admin(port: u16, api_key: Option<&str>) -> ConfigReloadResult {
    let url = format!("http://127.0.0.1:{port}/api/admin/reload");
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(client) => client,
        Err(error) => return ConfigReloadResult::RequestFailed(error.to_string()),
    };

    let mut request = client.post(&url);
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }

    match request.send() {
        Ok(response) => classify_admin_reload_response(response),
        Err(error) => ConfigReloadResult::RequestFailed(error.to_string()),
    }
}

fn classify_admin_reload_response(response: reqwest::blocking::Response) -> ConfigReloadResult {
    let status = response.status();
    let parsed_response = if status.is_success() {
        Some(
            response
                .json::<ReloadResponse>()
                .map_err(|error| error.to_string()),
        )
    } else {
        None
    };
    classify_admin_reload_parts(status.as_u16(), parsed_response)
}

fn classify_admin_reload_parts(
    status: u16,
    parsed_response: Option<Result<ReloadResponse, String>>,
) -> ConfigReloadResult {
    if status == 401 {
        return ConfigReloadResult::Unauthorized(
            "Admin API authentication failed. Check the UI config API key.".to_string(),
        );
    }
    if status == 404 {
        return ConfigReloadResult::AdminApiUnavailable(
            "Admin API reload is disabled or unavailable on the running server.".to_string(),
        );
    }
    if !(200..300).contains(&status) {
        return ConfigReloadResult::RequestFailed(format!("Admin API returned HTTP {status}"));
    }

    match parsed_response.expect("successful Admin reload responses must be parsed") {
        Ok(reload_response) if reload_response.success => {
            ConfigReloadResult::Success(reload_response.message)
        }
        Ok(reload_response) => ConfigReloadResult::ReloadFailed(reload_response.message),
        Err(error) => ConfigReloadResult::RequestFailed(format!(
            "Could not parse Admin API reload response: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::AppSettings;

    #[test]
    fn admin_reload_is_disabled_by_default() {
        assert!(!admin_reload_enabled(&Config::default()));
    }

    #[test]
    fn admin_reload_can_be_enabled_explicitly() {
        let config = Config {
            app_settings: AppSettings {
                admin_api_enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(admin_reload_enabled(&config));
    }

    #[test]
    fn reload_result_messages_distinguish_core_states() {
        assert_eq!(
            ConfigReloadResult::Success("Config reloaded successfully".to_string()).ui_message(),
            "✓ Config reloaded successfully"
        );
        assert_eq!(
            ConfigReloadResult::AdminApiUnavailable("Admin API reload is disabled".to_string())
                .ui_message(),
            "✗ Admin API reload is disabled"
        );
        assert_eq!(
            ConfigReloadResult::Unauthorized("Admin API authentication failed".to_string())
                .ui_message(),
            "✗ Admin API authentication failed"
        );
        assert_eq!(
            ConfigReloadResult::RequestFailed("connection refused".to_string()).ui_message(),
            "✗ connection refused"
        );
    }

    #[test]
    fn admin_reload_response_classifies_disabled_without_bare_http_status() {
        assert_eq!(
            classify_admin_reload_parts(404, None),
            ConfigReloadResult::AdminApiUnavailable(
                "Admin API reload is disabled or unavailable on the running server.".to_string()
            )
        );
    }

    #[test]
    fn admin_reload_response_classifies_auth_failure() {
        assert_eq!(
            classify_admin_reload_parts(401, None),
            ConfigReloadResult::Unauthorized(
                "Admin API authentication failed. Check the UI config API key.".to_string()
            )
        );
    }

    #[test]
    fn admin_reload_response_classifies_request_failure() {
        assert_eq!(
            classify_admin_reload_parts(500, None),
            ConfigReloadResult::RequestFailed("Admin API returned HTTP 500".to_string())
        );
    }

    #[test]
    fn admin_reload_response_classifies_success_body() {
        assert_eq!(
            classify_admin_reload_parts(
                200,
                Some(Ok(ReloadResponse {
                    success: true,
                    message: "Config reloaded successfully".to_string(),
                })),
            ),
            ConfigReloadResult::Success("Config reloaded successfully".to_string())
        );
    }

    #[test]
    fn admin_reload_response_classifies_failed_body() {
        assert_eq!(
            classify_admin_reload_parts(
                200,
                Some(Ok(ReloadResponse {
                    success: false,
                    message: "Reload failed: invalid config".to_string(),
                })),
            ),
            ConfigReloadResult::ReloadFailed("Reload failed: invalid config".to_string())
        );
    }

    #[test]
    fn admin_reload_response_classifies_parse_failure() {
        assert_eq!(
            classify_admin_reload_parts(200, Some(Err("expected value".to_string()))),
            ConfigReloadResult::RequestFailed(
                "Could not parse Admin API reload response: expected value".to_string()
            )
        );
    }
}
