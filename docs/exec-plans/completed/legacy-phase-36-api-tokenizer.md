# Phase 36 — API Tokenizer Backend

**状态：** 🚧 实施中
**优先级：** P2（功能增强）
**预计时间：** 2-3 小时

---

## 📋 目标

为 tokenizer 添加 API backend 支持，当本地 tokenizer（tiktoken/huggingface）不可用或出错时，可以通过 HTTP API 获取 token 计数。

---

## 🎯 背景

**当前状态：**
- ✅ 支持 Tiktoken backend（本地）
- ✅ 支持 Huggingface backend（本地）
- ❌ 无 API fallback
- ❌ 本地 tokenizer 失败时无备选方案

**需求：**
- 添加 API backend 选项
- 支持自定义 API endpoint
- 实现 HTTP 调用和缓存
- 错误处理和重试机制

---

## 📐 实现计划

### Phase 36.1: 扩展 TokenizerBackend 枚举

**时间：** 0.5 小时

**Step 1: 修改枚举定义**

```rust
// ccr-types/src/lib.rs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TokenizerBackend {
    Tiktoken,
    Huggingface,
    Api { endpoint: String },
}

impl Default for TokenizerBackend {
    fn default() -> Self {
        TokenizerBackend::Tiktoken
    }
}
```

**Step 2: 更新配置示例**

```json5
{
  "Router": {
    "default": "openai,gpt-4o",
    "tokenizer_backend": {
      "api": {
        "endpoint": "https://api.example.com/v1/tokenize"
      }
    }
  }
}
```

或者简化配置：

```json5
{
  "Router": {
    "tokenizer_backend": "tiktoken"  // 或 "huggingface"
  }
}
```

---

### Phase 36.2: 实现 API tokenizer

**时间：** 1 小时

**创建新模块：**

```rust
// ccr-router/src/tokenizer/api.rs

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error};

#[derive(Debug, Serialize)]
struct TokenizeRequest {
    text: String,
}

#[derive(Debug, Deserialize)]
struct TokenizeResponse {
    token_count: usize,
}

pub async fn count_tokens_api(text: &str, endpoint: &str) -> Result<usize, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let request_body = TokenizeRequest {
        text: text.to_string(),
    };

    debug!(endpoint = %endpoint, text_len = text.len(), "Calling API tokenizer");

    let response = client
        .post(endpoint)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        error!(status = %status, "API tokenizer returned error");
        return Err(format!("API returned status: {}", status));
    }

    let result: TokenizeResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    debug!(count = result.token_count, "API tokenizer result");

    Ok(result.token_count)
}
```

---

### Phase 36.3: 添加缓存层

**时间：** 0.5 小时

**简单缓存实现：**

```rust
// ccr-router/src/tokenizer/cache.rs

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

pub struct TokenCache {
    cache: Arc<RwLock<HashMap<u64, usize>>>,
    max_size: usize,
}

impl TokenCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
        }
    }

    pub fn get(&self, text: &str) -> Option<usize> {
        let hash = Self::hash_text(text);
        self.cache.read().ok()?.get(&hash).copied()
    }

    pub fn set(&self, text: &str, count: usize) {
        let hash = Self::hash_text(text);
        let mut cache = match self.cache.write() {
            Ok(c) => c,
            Err(_) => return,
        };

        // Simple eviction: clear cache if it's too large
        if cache.len() >= self.max_size {
            cache.clear();
        }

        cache.insert(hash, count);
    }

    fn hash_text(text: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }
}
```

---

### Phase 36.4: 集成到 count_tokens

**时间：** 0.5 小时

**修改 ccr-router/src/lib.rs:**

```rust
use ccr_types::{Config, MessagesRequest, TokenizerBackend};
use tokio::runtime::Runtime;
use once_cell::sync::Lazy;

mod tokenizer;
use tokenizer::api::count_tokens_api;
use tokenizer::cache::TokenCache;

static TOKEN_CACHE: Lazy<TokenCache> = Lazy::new(|| TokenCache::new(1000));

pub fn count_tokens(req: &MessagesRequest, backend: &TokenizerBackend) -> usize {
    let count = |text: &str| match backend {
        TokenizerBackend::Tiktoken => count_tokens_tiktoken(text),
        TokenizerBackend::Huggingface => count_tokens_hf(text),
        TokenizerBackend::Api { endpoint } => {
            // Check cache first
            if let Some(cached) = TOKEN_CACHE.get(text) {
                return cached;
            }

            // Call API
            let rt = Runtime::new().unwrap();
            match rt.block_on(count_tokens_api(text, endpoint)) {
                Ok(count) => {
                    TOKEN_CACHE.set(text, count);
                    count
                }
                Err(e) => {
                    eprintln!("API tokenizer failed: {}, falling back to tiktoken", e);
                    let count = count_tokens_tiktoken(text);
                    TOKEN_CACHE.set(text, count);
                    count
                }
            }
        }
    };

    let mut total = 0;
    for msg in &req.messages {
        total += count(&msg.content.to_string());
    }
    if let Some(sys) = &req.system {
        total += count(&sys.to_string());
    }
    total
}
```

---

### Phase 36.5: 依赖更新

**时间：** 0.5 小时

**ccr-router/Cargo.toml:**

```toml
[dependencies]
reqwest = { version = "0.12", features = ["json"] }
tokio = { version = "1.52", features = ["rt"] }
once_cell = "1.21"
tracing = "0.1.44"
```

---

## 🧪 测试计划

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_api_tokenizer() {
        // Mock API server would be needed
        let endpoint = "http://localhost:8080/tokenize";
        let text = "Hello world";

        // This would fail without a running mock server
        // Just for demonstration
        let result = count_tokens_api(text, endpoint).await;
        assert!(result.is_err() || result.unwrap() > 0);
    }

    #[test]
    fn test_cache() {
        let cache = TokenCache::new(10);
        let text = "test text";

        assert!(cache.get(text).is_none());
        cache.set(text, 42);
        assert_eq!(cache.get(text), Some(42));
    }
}
```

### 集成测试

```bash
# 1. 启动 mock API server (可选)
# python -m http.server 8080

# 2. 配置 API backend
# vim ~/.claude-code-router/config.json
{
  "Router": {
    "tokenizer_backend": {
      "api": {
        "endpoint": "http://localhost:8080/tokenize"
      }
    }
  }
}

# 3. 测试 token counting
ccr count-tokens '{"messages": [{"role": "user", "content": "test"}]}'
```

---

## 📊 完成标准

- [ ] TokenizerBackend 枚举扩展
- [ ] API tokenizer 实现
- [ ] 缓存层实现
- [ ] count_tokens 集成
- [ ] 依赖更新
- [ ] 错误处理和 fallback
- [ ] 单元测试
- [ ] 文档更新

---

## 📝 API 契约

### Request

```http
POST /tokenize HTTP/1.1
Content-Type: application/json

{
  "text": "The text to tokenize"
}
```

### Response (Success)

```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "token_count": 42
}
```

### Response (Error)

```http
HTTP/1.1 400 Bad Request
Content-Type: application/json

{
  "error": "Invalid request"
}
```

---

## 🔧 配置示例

### 使用 Tiktoken (默认)

```json5
{
  "Router": {
    "tokenizer_backend": "tiktoken"
  }
}
```

### 使用 Huggingface

```json5
{
  "Router": {
    "tokenizer_backend": "huggingface"
  }
}
```

### 使用 API

```json5
{
  "Router": {
    "tokenizer_backend": {
      "api": {
        "endpoint": "https://api.example.com/v1/tokenize"
      }
    }
  }
}
```

---

## 🔄 后续工作

- Phase 37: Config backup on Apply
- Phase 38: Proxy support

---

**创建时间:** 2026-04-28
**预计开始:** TBD
**预计完成:** TBD
