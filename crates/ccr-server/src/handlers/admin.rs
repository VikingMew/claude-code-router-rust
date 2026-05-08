use actix_web::{HttpRequest, HttpResponse, http::StatusCode, web};
use ccr_types::Config;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{AppState, auth_check};

#[derive(Debug, Serialize, Deserialize)]
pub struct ReloadResponse {
    pub success: bool,
    pub message: String,
}

/// POST /api/admin/reload
///
/// Manually reload configuration from disk
fn admin_api_rejection(config: &Config, req: &HttpRequest) -> Option<StatusCode> {
    if !config.app_settings.admin_api_enabled {
        return Some(StatusCode::NOT_FOUND);
    }
    if !auth_check(req, config) {
        return Some(StatusCode::UNAUTHORIZED);
    }
    None
}

pub async fn reload_config(req: HttpRequest, state: web::Data<Arc<AppState>>) -> HttpResponse {
    let config = state.get_config().await;
    if let Some(status) = admin_api_rejection(&config, &req) {
        return HttpResponse::build(status).finish();
    }

    match state.reload_config().await {
        Ok(_) => {
            println!("✅ Config reload successful via API");
            HttpResponse::Ok().json(ReloadResponse {
                success: true,
                message: "Config reloaded successfully".to_string(),
            })
        }
        Err(e) => {
            eprintln!("❌ Config reload failed via API: {}", e);
            HttpResponse::InternalServerError().json(ReloadResponse {
                success: false,
                message: format!("Reload failed: {}", e),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;
    use ccr_types::AppSettings;

    #[test]
    fn test_reload_response_format() {
        let response = ReloadResponse {
            success: true,
            message: "Test message".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("Test message"));
    }

    #[test]
    fn admin_api_is_rejected_when_disabled() {
        let config = Config::default();
        let req = TestRequest::default()
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .to_http_request();

        assert_eq!(
            admin_api_rejection(&config, &req),
            Some(StatusCode::NOT_FOUND)
        );
    }

    #[test]
    fn admin_api_requires_auth_when_enabled() {
        let config = Config {
            api_key: Some("secret".into()),
            app_settings: AppSettings {
                admin_api_enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let wrong_req = TestRequest::default()
            .insert_header(("authorization", "Bearer wrong"))
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .to_http_request();
        let right_req = TestRequest::default()
            .insert_header(("authorization", "Bearer secret"))
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .to_http_request();

        assert_eq!(
            admin_api_rejection(&config, &wrong_req),
            Some(StatusCode::UNAUTHORIZED)
        );
        assert_eq!(admin_api_rejection(&config, &right_req), None);
    }
}
