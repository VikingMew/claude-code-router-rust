use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic() {
        let cache = TokenCache::new(10);
        let text = "test text";

        assert!(cache.get(text).is_none());
        cache.set(text, 42);
        assert_eq!(cache.get(text), Some(42));
    }

    #[test]
    fn test_cache_eviction() {
        let cache = TokenCache::new(2);

        cache.set("text1", 10);
        cache.set("text2", 20);

        assert_eq!(cache.get("text1"), Some(10));
        assert_eq!(cache.get("text2"), Some(20));

        // Adding third item should trigger eviction
        cache.set("text3", 30);

        // After eviction, old entries might be gone
        // (our simple implementation clears everything)
        assert_eq!(cache.get("text3"), Some(30));
    }
}
