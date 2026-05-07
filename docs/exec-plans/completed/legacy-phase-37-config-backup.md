# Phase 37 — Config Backup on Apply

**状态：** 🚧 实施中
**优先级：** P2（数据安全）
**预计时间：** 2-3 小时

---

## 📋 目标

在用户通过UI或API应用配置更改时，自动创建配置文件备份，支持回滚到之前的版本。

---

## 🎯 背景

**当前状态：**
- ✅ 配置保存功能存在
- ✅ 配置重载功能存在
- ❌ 无自动备份机制
- ❌ 配置损坏后无法恢复

**需求：**
- 应用配置前自动备份
- 保留最近N个备份（默认5个）
- 提供备份列表和恢复功能
- UI显示备份历史

---

## 📐 实现计划

### Phase 37.1: 备份模块 (`ccr-config/src/backup.rs`)

**时间：** 1 小时

**功能：**
- 创建备份文件（带时间戳）
- 列出所有备份
- 恢复指定备份
- 清理旧备份（保留最新N个）

**实现：**

```rust
// ccr-config/src/backup.rs

use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use chrono::Utc;

const MAX_BACKUPS: usize = 5;

/// Create a backup of the config file
pub fn create_backup(config_path: &Path) -> Result<PathBuf> {
    if !config_path.exists() {
        return Err(anyhow::anyhow!("Config file does not exist"));
    }

    let backup_dir = get_backup_dir()?;
    fs::create_dir_all(&backup_dir)?;

    // Generate backup filename with timestamp
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("config.{}.json", timestamp);
    let backup_path = backup_dir.join(&backup_name);

    // Copy config to backup
    fs::copy(config_path, &backup_path)
        .context("Failed to create backup")?;

    // Clean up old backups
    cleanup_old_backups(&backup_dir, MAX_BACKUPS)?;

    Ok(backup_path)
}

/// List all backup files (sorted by creation time, newest first)
pub fn list_backups() -> Result<Vec<BackupInfo>> {
    let backup_dir = get_backup_dir()?;

    if !backup_dir.exists() {
        return Ok(vec![]);
    }

    let mut backups = Vec::new();

    for entry in fs::read_dir(&backup_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("config.") {
                    let metadata = fs::metadata(&path)?;
                    let modified = metadata.modified()?;

                    backups.push(BackupInfo {
                        path: path.clone(),
                        name: name.to_string(),
                        created: modified,
                    });
                }
            }
        }
    }

    // Sort by creation time (newest first)
    backups.sort_by(|a, b| b.created.cmp(&a.created));

    Ok(backups)
}

/// Restore a backup to the main config file
pub fn restore_backup(backup_path: &Path, config_path: &Path) -> Result<()> {
    if !backup_path.exists() {
        return Err(anyhow::anyhow!("Backup file does not exist"));
    }

    // Create a backup of current config before restoring
    if config_path.exists() {
        create_backup(config_path)?;
    }

    // Restore backup
    fs::copy(backup_path, config_path)
        .context("Failed to restore backup")?;

    Ok(())
}

/// Delete a specific backup file
pub fn delete_backup(backup_path: &Path) -> Result<()> {
    if !backup_path.exists() {
        return Err(anyhow::anyhow!("Backup file does not exist"));
    }

    fs::remove_file(backup_path)
        .context("Failed to delete backup")?;

    Ok(())
}

fn get_backup_dir() -> Result<PathBuf> {
    let config_dir = dirs_next::home_dir()
        .context("Could not find home directory")?
        .join(".claude-code-router");

    Ok(config_dir.join("backups"))
}

fn cleanup_old_backups(backup_dir: &Path, max_keep: usize) -> Result<()> {
    let mut backups = list_backups()?;

    // Keep only the newest max_keep backups
    if backups.len() > max_keep {
        for backup in backups.drain(max_keep..) {
            fs::remove_file(&backup.path).ok();
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub name: String,
    pub created: std::time::SystemTime,
}
```

---

### Phase 37.2: 集成到配置保存 (`ccr-config/src/lib.rs`)

**时间：** 0.5 小时

**修改 `save_config` 函数：**

```rust
// ccr-config/src/lib.rs

pub mod backup;

use backup::create_backup;

pub fn save_config(config: &Config, path: &Path) -> anyhow::Result<()> {
    // Create backup before saving
    if path.exists() {
        match create_backup(path) {
            Ok(backup_path) => {
                println!("Created backup: {}", backup_path.display());
            }
            Err(e) => {
                eprintln!("Warning: Failed to create backup: {}", e);
                // Continue saving even if backup fails
            }
        }
    }

    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(path, json)?;
    Ok(())
}
```

---

### Phase 37.3: API端点 (`ccr-server/src/handlers/backup.rs`)

**时间：** 0.5 小时

**创建新handler模块：**

```rust
// ccr-server/src/handlers/backup.rs

use actix_web::{web, HttpRequest, HttpResponse};
use ccr_config::backup::{list_backups, restore_backup, delete_backup, BackupInfo};
use ccr_config::default_config_path;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use crate::AppState;

#[derive(Serialize)]
struct BackupListResponse {
    backups: Vec<BackupItem>,
}

#[derive(Serialize)]
struct BackupItem {
    name: String,
    path: String,
    created: String,
}

#[derive(Deserialize)]
struct RestoreRequest {
    backup_name: String,
}

pub async fn list_backups_handler(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !crate::auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }

    match list_backups() {
        Ok(backups) => {
            let items: Vec<BackupItem> = backups
                .into_iter()
                .map(|b| BackupItem {
                    name: b.name,
                    path: b.path.to_string_lossy().to_string(),
                    created: format!("{:?}", b.created),
                })
                .collect();

            HttpResponse::Ok().json(BackupListResponse { backups: items })
        }
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn restore_backup_handler(
    req: HttpRequest,
    body: web::Json<RestoreRequest>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !crate::auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }

    let backup_dir = dirs_next::home_dir()
        .unwrap_or_default()
        .join(".claude-code-router")
        .join("backups");

    let backup_path = backup_dir.join(&body.backup_name);
    let config_path = default_config_path();

    match restore_backup(&backup_path, &config_path) {
        Ok(_) => {
            // Reload config after restore
            if let Err(e) = state.reload_config().await {
                return HttpResponse::InternalServerError()
                    .body(format!("Restored but failed to reload: {}", e));
            }

            HttpResponse::Ok().json(serde_json::json!({"success": true}))
        }
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn delete_backup_handler(
    req: HttpRequest,
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !crate::auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }

    let backup_name = path.into_inner();
    let backup_dir = dirs_next::home_dir()
        .unwrap_or_default()
        .join(".claude-code-router")
        .join("backups");

    let backup_path = backup_dir.join(&backup_name);

    match delete_backup(&backup_path) {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}
```

**注册路由 (`ccr-server/src/main.rs`)：**

```rust
.route("/api/backups", web::get().to(handlers::backup::list_backups_handler))
.route("/api/backups/restore", web::post().to(handlers::backup::restore_backup_handler))
.route("/api/backups/{name}", web::delete().to(handlers::backup::delete_backup_handler))
```

---

### Phase 37.4: UI备份管理 (`ccr-ui/src/backup_tab.rs`)

**时间：** 1 小时

**创建新Tab：**

```rust
// ccr-ui/src/backup_tab.rs

use eframe::egui;
use reqwest::blocking::Client;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct BackupItem {
    name: String,
    path: String,
    created: String,
}

#[derive(Debug, Deserialize)]
struct BackupListResponse {
    backups: Vec<BackupItem>,
}

pub struct BackupTab {
    backups: Vec<BackupItem>,
    status: String,
    loading: bool,
}

impl BackupTab {
    pub fn new() -> Self {
        let mut tab = Self {
            backups: vec![],
            status: String::new(),
            loading: false,
        };
        tab.refresh_backups();
        tab
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("Configuration Backups");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("🔄 Refresh").clicked() {
                self.refresh_backups();
            }

            if self.loading {
                ui.spinner();
            }
        });

        if !self.status.is_empty() {
            ui.add_space(8.0);
            ui.colored_label(
                if self.status.starts_with("✅") {
                    egui::Color32::GREEN
                } else {
                    egui::Color32::RED
                },
                &self.status,
            );
        }

        ui.add_space(16.0);

        if self.backups.is_empty() {
            ui.label("No backups found.");
        } else {
            self.show_backup_table(ui);
        }
    }

    fn show_backup_table(&mut self, ui: &mut egui::Ui) {
        use egui_extras::{Column, TableBuilder};

        TableBuilder::new(ui)
            .striped(true)
            .column(Column::auto())
            .column(Column::remainder())
            .column(Column::auto())
            .column(Column::auto())
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Backup Name");
                });
                header.col(|ui| {
                    ui.strong("Created");
                });
                header.col(|ui| {
                    ui.strong("Restore");
                });
                header.col(|ui| {
                    ui.strong("Delete");
                });
            })
            .body(|mut body| {
                for backup in &self.backups.clone() {
                    body.row(24.0, |mut row| {
                        row.col(|ui| {
                            ui.label(&backup.name);
                        });
                        row.col(|ui| {
                            ui.label(&backup.created);
                        });
                        row.col(|ui| {
                            if ui.button("⬅ Restore").clicked() {
                                self.restore_backup(&backup.name);
                            }
                        });
                        row.col(|ui| {
                            if ui.button("🗑 Delete").clicked() {
                                self.delete_backup(&backup.name);
                            }
                        });
                    });
                }
            });
    }

    fn refresh_backups(&mut self) {
        self.loading = true;
        self.status = "Loading...".to_string();

        let client = Client::new();
        match client.get("http://127.0.0.1:3456/api/backups").send() {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<BackupListResponse>() {
                    Ok(data) => {
                        self.backups = data.backups;
                        self.status = format!("✅ Loaded {} backup(s)", self.backups.len());
                    }
                    Err(e) => {
                        self.status = format!("❌ Parse error: {}", e);
                    }
                }
            }
            Ok(resp) => {
                self.status = format!("❌ HTTP {}", resp.status());
            }
            Err(e) => {
                self.status = format!("❌ Request failed: {}", e);
            }
        }

        self.loading = false;
    }

    fn restore_backup(&mut self, backup_name: &str) {
        self.status = format!("Restoring {}...", backup_name);

        let client = Client::new();
        let body = serde_json::json!({"backup_name": backup_name});

        match client
            .post("http://127.0.0.1:3456/api/backups/restore")
            .json(&body)
            .send()
        {
            Ok(resp) if resp.status().is_success() => {
                self.status = format!("✅ Restored {}", backup_name);
                self.refresh_backups();
            }
            Ok(resp) => {
                self.status = format!("❌ Restore failed: HTTP {}", resp.status());
            }
            Err(e) => {
                self.status = format!("❌ Restore failed: {}", e);
            }
        }
    }

    fn delete_backup(&mut self, backup_name: &str) {
        self.status = format!("Deleting {}...", backup_name);

        let client = Client::new();
        let url = format!("http://127.0.0.1:3456/api/backups/{}", backup_name);

        match client.delete(&url).send() {
            Ok(resp) if resp.status().is_success() => {
                self.status = format!("✅ Deleted {}", backup_name);
                self.refresh_backups();
            }
            Ok(resp) => {
                self.status = format!("❌ Delete failed: HTTP {}", resp.status());
            }
            Err(e) => {
                self.status = format!("❌ Delete failed: {}", e);
            }
        }
    }
}
```

---

## 🧪 测试计划

### 单元测试 (`ccr-config/src/backup.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_create_and_list_backup() {
        // Create temp config file
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        fs::write(&config_path, "{\"test\": true}").unwrap();

        // Create backup
        let backup_path = create_backup(&config_path).unwrap();
        assert!(backup_path.exists());

        // List backups
        let backups = list_backups().unwrap();
        assert!(!backups.is_empty());
    }

    #[test]
    fn test_restore_backup() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Create initial config
        fs::write(&config_path, "{\"version\": 1}").unwrap();

        // Create backup
        let backup_path = create_backup(&config_path).unwrap();

        // Modify config
        fs::write(&config_path, "{\"version\": 2}").unwrap();

        // Restore backup
        restore_backup(&backup_path, &config_path).unwrap();

        // Verify restored content
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("\"version\": 1"));
    }
}
```

---

## 📊 完成标准

- [ ] backup.rs 模块实现
- [ ] create_backup 函数
- [ ] list_backups 函数
- [ ] restore_backup 函数
- [ ] delete_backup 函数
- [ ] cleanup_old_backups 逻辑
- [ ] 集成到 save_config
- [ ] API端点（list, restore, delete）
- [ ] UI备份管理Tab
- [ ] 单元测试
- [ ] 集成测试
- [ ] 文档更新

---

## 📝 API文档

### GET /api/backups

列出所有备份

**Response:**
```json
{
  "backups": [
    {
      "name": "config.20260428_143022.json",
      "path": "/Users/xxx/.claude-code-router/backups/config.20260428_143022.json",
      "created": "2026-04-28T14:30:22Z"
    }
  ]
}
```

### POST /api/backups/restore

恢复指定备份

**Request:**
```json
{
  "backup_name": "config.20260428_143022.json"
}
```

**Response:**
```json
{
  "success": true
}
```

### DELETE /api/backups/{name}

删除指定备份

**Response:**
```json
{
  "success": true
}
```

---

## 🔧 配置

### 备份存储位置

```
~/.claude-code-router/backups/
├── config.20260428_143022.json
├── config.20260428_142510.json
├── config.20260428_141305.json
├── config.20260428_135540.json
└── config.20260428_132215.json
```

### 备份保留策略

- 默认保留最新 5 个备份
- 可通过修改 `MAX_BACKUPS` 常量调整
- 按创建时间排序，自动清理最旧的备份

---

## 🔄 下一步

- Phase 38: Proxy support

---

**创建时间:** 2026-04-28
**预计开始:** TBD
**预计完成:** TBD
