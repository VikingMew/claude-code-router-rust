use lru::LruCache;
use sha2::{Digest, Sha256};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cache entry
#[derive(Clone)]
struct CacheEntry {
    result: String,
    timestamp: Instant,
}

/// Image analysis cache
pub struct ImageCache {
    cache: Arc<RwLock<LruCache<String, CacheEntry>>>,
    ttl: Duration,
}

impl ImageCache {
    /// Create cache with capacity and TTL
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap(),
            ))),
            ttl,
        }
    }

    /// Compute hash of image data and prompt
    fn compute_hash(data: &[u8], prompt: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.update(prompt.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Get from cache
    pub async fn get(&self, data: &[u8], prompt: &str) -> Option<String> {
        let key = Self::compute_hash(data, prompt);
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.get(&key) {
            // Check expiry
            if entry.timestamp.elapsed() < self.ttl {
                log::debug!("Cache hit for image: {}", &key[..8]);
                return Some(entry.result.clone());
            } else {
                log::debug!("Cache expired for image: {}", &key[..8]);
                cache.pop(&key);
            }
        }

        None
    }

    /// Put into cache
    pub async fn put(&self, data: &[u8], prompt: &str, result: String) {
        let key = Self::compute_hash(data, prompt);
        let mut cache = self.cache.write().await;

        cache.put(
            key.clone(),
            CacheEntry {
                result,
                timestamp: Instant::now(),
            },
        );

        log::debug!("Cached result for image: {}", &key[..8]);
    }

    /// Clear cache
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        log::info!("Image cache cleared");
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        CacheStats {
            size: cache.len(),
            capacity: cache.cap().get(),
        }
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        // Default: 100 entries, 5 minutes TTL
        Self::new(100, Duration::from_secs(300))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheStats {
    pub size: usize,
    pub capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_hit() {
        let cache = ImageCache::new(10, Duration::from_secs(60));

        let image_data = b"fake image data";
        let prompt = "What is this?";
        let result = "A fake image".to_string();

        // Write to cache
        cache.put(image_data, prompt, result.clone()).await;

        // Read from cache
        let cached = cache.get(image_data, prompt).await;
        assert_eq!(cached, Some(result));
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = ImageCache::new(10, Duration::from_secs(60));

        let cached = cache.get(b"unknown", "prompt").await;
        assert_eq!(cached, None);
    }

    #[tokio::test]
    async fn test_cache_expiry() {
        let cache = ImageCache::new(10, Duration::from_millis(100));

        let image_data = b"fake image data";
        let prompt = "What is this?";
        let result = "A fake image".to_string();

        cache.put(image_data, prompt, result).await;

        // Wait for expiry
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Cache should be expired
        let cached = cache.get(image_data, prompt).await;
        assert_eq!(cached, None);
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let cache = ImageCache::new(2, Duration::from_secs(60));

        // Write 3 entries, capacity is 2
        cache.put(b"image1", "prompt", "result1".to_string()).await;
        cache.put(b"image2", "prompt", "result2".to_string()).await;
        cache.put(b"image3", "prompt", "result3".to_string()).await;

        // image1 should be evicted
        let cached1 = cache.get(b"image1", "prompt").await;
        assert_eq!(cached1, None);

        // image2 and image3 should still be there
        let cached2 = cache.get(b"image2", "prompt").await;
        assert!(cached2.is_some());

        let cached3 = cache.get(b"image3", "prompt").await;
        assert!(cached3.is_some());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = ImageCache::new(10, Duration::from_secs(60));

        let stats = cache.stats().await;
        assert_eq!(stats.size, 0);
        assert_eq!(stats.capacity, 10);

        cache.put(b"image1", "prompt", "result1".to_string()).await;
        let stats = cache.stats().await;
        assert_eq!(stats.size, 1);
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let cache = ImageCache::new(10, Duration::from_secs(60));

        cache.put(b"image1", "prompt", "result1".to_string()).await;
        cache.put(b"image2", "prompt", "result2".to_string()).await;

        cache.clear().await;

        let stats = cache.stats().await;
        assert_eq!(stats.size, 0);
    }

    #[test]
    fn test_hash_consistency() {
        let hash1 = ImageCache::compute_hash(b"data", "prompt");
        let hash2 = ImageCache::compute_hash(b"data", "prompt");
        assert_eq!(hash1, hash2);

        let hash3 = ImageCache::compute_hash(b"different", "prompt");
        assert_ne!(hash1, hash3);
    }
}
