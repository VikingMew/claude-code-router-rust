# Phase 38 — Proxy Support

**状态：** 🚧 实施中
**优先级：** P2（网络功能）
**预计时间：** 1-2 小时

---

## 📋 目标

为HTTP客户端添加代理支持，允许通过HTTP/HTTPS/SOCKS5代理访问LLM API。

---

## 🎯 背景

**当前状态：**
- ✅ reqwest HTTP客户端
- ❌ 无代理配置
- ❌ 企业环境/防火墙无法使用

**需求：**
- 支持HTTP/HTTPS代理
- 支持SOCKS5代理
- 支持代理认证
- 配置灵活（全局/per-provider）

---

## 📐 实现计划

### Phase 38.1: 配置结构扩展

**时间：** 0.5 小时

**Step 1: 添加 ProxyConfig 到 Config**

```rust
// ccr-types/src/lib.rs

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
```

**配置示例：**

```json5
{
  "Providers": [...],
  "Router": {...},
  "Proxy": {
    "http": "http://proxy.example.com:8080",
    "https": "http://proxy.example.com:8080",
    "no_proxy": ["localhost", "127.0.0.1"]
  }
}
```

或使用SOCKS5：

```json5
{
  "Proxy": {
    "socks5": "socks5://127.0.0.1:1080"
  }
}
```

带认证：

```json5
{
  "Proxy": {
    "http": "http://user:password@proxy.example.com:8080"
  }
}
```

---

### Phase 38.2: HTTP客户端配置

**时间：** 0.5 小时

**修改 AppState 初始化：**

```rust
// ccr-server/src/main.rs

use reqwest::{Client, Proxy};

fn build_http_client(config: &Config) -> reqwest::Result<Client> {
    let mut client_builder = Client::builder()
        .timeout(Duration::from_secs(300));

    // Configure proxy if present
    if let Some(proxy_config) = &config.proxy {
        if let Some(http_proxy) = &proxy_config.http {
            let proxy = Proxy::http(http_proxy)?;
            client_builder = client_builder.proxy(proxy);
        }

        if let Some(https_proxy) = &proxy_config.https {
            let proxy = Proxy::https(https_proxy)?;
            client_builder = client_builder.proxy(proxy);
        }

        if let Some(socks5_proxy) = &proxy_config.socks5 {
            let proxy = Proxy::all(socks5_proxy)?;
            client_builder = client_builder.proxy(proxy);
        }

        if let Some(no_proxy) = &proxy_config.no_proxy {
            for domain in no_proxy {
                let proxy = Proxy::custom(move |url| {
                    if url.host_str() == Some(domain) {
                        None
                    } else {
                        // Use configured proxy
                        // This is simplified, actual implementation needs more logic
                        None
                    }
                });
                client_builder = client_builder.proxy(proxy);
            }
        }
    }

    client_builder.build()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_tracing();

    let config_path = default_config_path();
    let config = load_config(&config_path).unwrap_or_else(|e| {
        warn!("Could not load config: {}", e);
        Config::default()
    });

    // Build HTTP client with proxy support
    let client = build_http_client(&config).unwrap_or_else(|e| {
        warn!("Failed to configure proxy: {}, using default client", e);
        Client::new()
    });

    let state = Arc::new(AppState {
        reloadable_config,
        transformers: Arc::new(TransformerRegistry::new()),
        client,  // Use configured client
        agents,
    });

    // ... rest of server setup
}
```

---

### Phase 38.3: 环境变量支持

**时间：** 0.5 小时

**支持标准环境变量：**

```rust
// ccr-config/src/lib.rs

pub fn load_config(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;
    let interpolated = interpolate_env(&raw);
    let mut config: Config = json5::from_str(&interpolated).context("Failed to parse config")?;

    // Load proxy from environment variables if not configured
    if config.proxy.is_none() {
        config.proxy = load_proxy_from_env();
    }

    Ok(config)
}

fn load_proxy_from_env() -> Option<ProxyConfig> {
    let http = std::env::var("HTTP_PROXY").ok()
        .or_else(|| std::env::var("http_proxy").ok());
    let https = std::env::var("HTTPS_PROXY").ok()
        .or_else(|| std::env::var("https_proxy").ok());
    let socks5 = std::env::var("SOCKS5_PROXY").ok()
        .or_else(|| std::env::var("socks5_proxy").ok());
    let no_proxy = std::env::var("NO_PROXY").ok()
        .or_else(|| std::env::var("no_proxy").ok())
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect());

    if http.is_some() || https.is_some() || socks5.is_some() {
        Some(ProxyConfig {
            http,
            https,
            socks5,
            no_proxy,
        })
    } else {
        None
    }
}
```

---

## 🧪 测试计划

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_config_parsing() {
        let json = r#"{
            "Providers": [],
            "Router": {"default": "test"},
            "Proxy": {
                "http": "http://proxy.example.com:8080"
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.proxy.is_some());
        assert_eq!(
            config.proxy.unwrap().http,
            Some("http://proxy.example.com:8080".to_string())
        );
    }

    #[test]
    fn test_proxy_from_env() {
        std::env::set_var("HTTP_PROXY", "http://env-proxy:8080");
        let proxy = load_proxy_from_env();
        assert!(proxy.is_some());
        assert_eq!(proxy.unwrap().http, Some("http://env-proxy:8080".to_string()));
        std::env::remove_var("HTTP_PROXY");
    }
}
```

### 集成测试

```bash
# 1. 配置HTTP代理
vim ~/.claude-code-router/config.json
{
  "Proxy": {
    "http": "http://localhost:8888"
  }
}

# 2. 启动代理服务器（如mitmproxy）
mitmproxy -p 8888

# 3. 启动CCR服务器
ccr-server

# 4. 发送请求，检查代理日志
curl -X POST http://localhost:3456/v1/messages \
  -H "Content-Type: application/json" \
  -d '{"model": "test", "messages": [{"role": "user", "content": "hello"}]}'

# 5. 在mitmproxy中应该能看到请求
```

---

## 📊 完成标准

- [ ] ProxyConfig 结构体定义
- [ ] Config 集成 proxy 字段
- [ ] build_http_client 函数
- [ ] 环境变量支持
- [ ] no_proxy 支持
- [ ] 单元测试
- [ ] 文档更新

---

## 📝 配置示例

### HTTP代理

```json5
{
  "Proxy": {
    "http": "http://proxy.company.com:8080",
    "https": "http://proxy.company.com:8080"
  }
}
```

### SOCKS5代理

```json5
{
  "Proxy": {
    "socks5": "socks5://127.0.0.1:1080"
  }
}
```

### 带认证的代理

```json5
{
  "Proxy": {
    "http": "http://username:password@proxy.company.com:8080"
  }
}
```

### 排除代理（no_proxy）

```json5
{
  "Proxy": {
    "http": "http://proxy.company.com:8080",
    "no_proxy": ["localhost", "127.0.0.1", "*.internal.com"]
  }
}
```

### 使用环境变量

```bash
export HTTP_PROXY=http://proxy.company.com:8080
export HTTPS_PROXY=http://proxy.company.com:8080
export NO_PROXY=localhost,127.0.0.1

ccr-server
```

---

## 🔧 代理类型

| 类型 | URL格式 | 示例 |
|------|---------|------|
| HTTP | `http://host:port` | `http://proxy.example.com:8080` |
| HTTPS | `http://host:port` | `http://proxy.example.com:8080` |
| SOCKS5 | `socks5://host:port` | `socks5://127.0.0.1:1080` |
| 认证 | `http://user:pass@host:port` | `http://user:pass@proxy.example.com:8080` |

---

## 🔄 下一步

- Phase 39: macOS installer

---

**创建时间:** 2026-04-28
**预计开始:** TBD
**预计完成:** TBD
