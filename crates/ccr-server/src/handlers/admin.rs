use actix_web::{HttpRequest, HttpResponse, web};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct ReloadResponse {
    pub success: bool,
    pub message: String,
}

/// POST /api/admin/reload
///
/// Manually reload configuration from disk
pub async fn reload_config(_req: HttpRequest, state: web::Data<Arc<AppState>>) -> HttpResponse {
    // TODO: Add auth check when integrating with main.rs

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
}
