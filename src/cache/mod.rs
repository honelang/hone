//! Content-addressed build cache for Hone
//!
//! Uses SHA256 hashing of source content, variant selections, args, and format
//! to cache compilation results on disk. Cache is stored at ~/.cache/hone/v1/.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::errors::{HoneError, HoneResult};

/// Cache key computed from compilation inputs
#[derive(Debug, Clone)]
pub struct CacheKey {
    /// The hex-encoded SHA256 hash
    pub hash: String,
}

impl CacheKey {
    /// Compute a cache key from compilation inputs
    pub fn compute(
        source_hashes: &[String],
        variants: &HashMap<String, String>,
        args_hash: Option<&str>,
        format: &str,
        hone_version: &str,
    ) -> Self {
        let mut hasher = Sha256::new();

        // Hash source content (includes all imports in order)
        for h in source_hashes {
            hasher.update(h.as_bytes());
            hasher.update(b"\x00");
        }

        // Hash variant selections (sorted for determinism)
        let mut variant_pairs: Vec<_> = variants.iter().collect();
        variant_pairs.sort_by_key(|(k, _)| *k);
        for (k, v) in variant_pairs {
            hasher.update(b"variant:");
            hasher.update(k.as_bytes());
            hasher.update(b"=");
            hasher.update(v.as_bytes());
            hasher.update(b"\x00");
        }

        // Hash args
        if let Some(ah) = args_hash {
            hasher.update(b"args:");
            hasher.update(ah.as_bytes());
            hasher.update(b"\x00");
        }

        // Hash format
        hasher.update(b"format:");
        hasher.update(format.as_bytes());
        hasher.update(b"\x00");

        // Hash compiler version
        hasher.update(b"version:");
        hasher.update(hone_version.as_bytes());

        let result = hasher.finalize();
        CacheKey {
            hash: hex_encode(&result),
        }
    }

    /// Compute SHA256 of a string
    pub fn hash_string(s: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(s.as_bytes());
        hex_encode(&hasher.finalize())
    }
}

/// Build cache with filesystem storage
pub struct BuildCache {
    /// Root directory for cache storage
    cache_dir: PathBuf,
}

impl BuildCache {
    /// Create a new build cache using the default directory (~/.cache/hone/v1/)
    pub fn new() -> Option<Self> {
        let cache_dir = default_cache_dir()?;
        Some(Self { cache_dir })
    }

    /// Create a build cache at a specific directory (for testing)
    pub fn with_dir(dir: PathBuf) -> Self {
        Self { cache_dir: dir }
    }

    /// Look up a cached result by key
    pub fn get(&self, key: &CacheKey) -> Option<CachedResult> {
        let path = self.entry_path(&key.hash);
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Store a compilation result
    pub fn put(&self, key: &CacheKey, result: &CachedResult) -> HoneResult<()> {
        let path = self.entry_path(&key.hash);

        // Create parent directory
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| HoneError::io_error(format!("failed to create cache dir: {}", e)))?;
        }

        // Write to temp file then rename (atomic)
        let tmp_path = path.with_extension("tmp");
        let content = serde_json::to_string(result)
            .map_err(|e| HoneError::io_error(format!("failed to serialize cache entry: {}", e)))?;

        std::fs::write(&tmp_path, &content)
            .map_err(|e| HoneError::io_error(format!("failed to write cache entry: {}", e)))?;

        std::fs::rename(&tmp_path, &path)
            .map_err(|e| HoneError::io_error(format!("failed to rename cache entry: {}", e)))?;

        Ok(())
    }

    /// Remove all cached entries
    pub fn clean(&self) -> HoneResult<usize> {
        if !self.cache_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        self.clean_recursive(&self.cache_dir, &mut count, None)?;
        Ok(count)
    }

    /// Remove cached entries older than the given duration
    pub fn clean_older_than(&self, max_age: std::time::Duration) -> HoneResult<usize> {
        if !self.cache_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        self.clean_recursive(&self.cache_dir, &mut count, Some(max_age))?;
        Ok(count)
    }

    /// Get the cache directory path
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    fn entry_path(&self, hash: &str) -> PathBuf {
        // Shard by first 2 hex chars for filesystem friendliness
        let (prefix, _) = hash.split_at(2.min(hash.len()));
        self.cache_dir.join(prefix).join(format!("{}.json", hash))
    }

    fn clean_recursive(
        &self,
        dir: &Path,
        count: &mut usize,
        max_age: Option<std::time::Duration>,
    ) -> HoneResult<()> {
        let entries = std::fs::read_dir(dir)
            .map_err(|e| HoneError::io_error(format!("failed to read cache dir: {}", e)))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| HoneError::io_error(format!("failed to read cache entry: {}", e)))?;

            let path = entry.path();

            if path.is_dir() {
                self.clean_recursive(&path, count, max_age)?;
                // Remove empty directories
                if std::fs::read_dir(&path)
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false)
                {
                    let _ = std::fs::remove_dir(&path);
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let should_remove = if let Some(age) = max_age {
                    entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|modified| {
                            std::time::SystemTime::now().duration_since(modified).ok()
                        })
                        .map(|file_age| file_age > age)
                        .unwrap_or(false)
                } else {
                    true
                };

                if should_remove && std::fs::remove_file(&path).is_ok() {
                    *count += 1;
                }
            }
        }

        Ok(())
    }
}

/// A cached compilation result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedResult {
    /// The compiled output string
    pub output: String,
    /// Output format used
    pub format: String,
    /// Source file path (for display)
    pub source_path: Option<String>,
    /// Timestamp when cached
    pub timestamp: u64,
    /// Hone version that produced this cache entry
    pub hone_version: String,
}

impl CachedResult {
    /// Create a new cache result
    pub fn new(output: String, format: &str, source_path: Option<&str>) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            output,
            format: format.to_string(),
            source_path: source_path.map(|s| s.to_string()),
            timestamp,
            hone_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Get the default cache directory
fn default_cache_dir() -> Option<PathBuf> {
    // Try XDG_CACHE_HOME first, then ~/.cache
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        Some(PathBuf::from(xdg).join("hone").join("v1"))
    } else if let Ok(home) = std::env::var("HOME") {
        Some(PathBuf::from(home).join(".cache").join("hone").join("v1"))
    } else {
        dirs_fallback()
    }
}

#[cfg(not(target_os = "windows"))]
fn dirs_fallback() -> Option<PathBuf> {
    None
}

#[cfg(target_os = "windows")]
fn dirs_fallback() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .map(|d| PathBuf::from(d).join("hone").join("cache").join("v1"))
}

/// Hex-encode bytes (no external dependency needed)
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Parse a duration string like "7d", "24h", "30m"
pub fn parse_duration(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_str, unit) = if let Some(stripped) = s.strip_suffix('d') {
        (stripped, "d")
    } else if let Some(stripped) = s.strip_suffix('h') {
        (stripped, "h")
    } else if let Some(stripped) = s.strip_suffix('m') {
        (stripped, "m")
    } else if let Some(stripped) = s.strip_suffix('s') {
        (stripped, "s")
    } else {
        // Default to seconds
        (s, "s")
    };

    let num: u64 = num_str.parse().ok()?;
    let secs = match unit {
        "d" => num * 86400,
        "h" => num * 3600,
        "m" => num * 60,
        "s" => num,
        _ => return None,
    };

    Some(std::time::Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cache_key_deterministic() {
        let sources = vec!["hash1".to_string(), "hash2".to_string()];
        let variants = HashMap::new();

        let key1 = CacheKey::compute(&sources, &variants, None, "json", "0.1.0");
        let key2 = CacheKey::compute(&sources, &variants, None, "json", "0.1.0");

        assert_eq!(key1.hash, key2.hash);
    }

    #[test]
    fn test_cache_key_changes_with_source() {
        let variants = HashMap::new();

        let key1 = CacheKey::compute(&["source_a".to_string()], &variants, None, "json", "0.1.0");
        let key2 = CacheKey::compute(&["source_b".to_string()], &variants, None, "json", "0.1.0");

        assert_ne!(key1.hash, key2.hash);
    }

    #[test]
    fn test_cache_key_changes_with_variant() {
        let sources = vec!["hash1".to_string()];

        let mut variants_a = HashMap::new();
        variants_a.insert("env".to_string(), "dev".to_string());

        let mut variants_b = HashMap::new();
        variants_b.insert("env".to_string(), "prod".to_string());

        let key1 = CacheKey::compute(&sources, &variants_a, None, "json", "0.1.0");
        let key2 = CacheKey::compute(&sources, &variants_b, None, "json", "0.1.0");

        assert_ne!(key1.hash, key2.hash);
    }

    #[test]
    fn test_cache_key_changes_with_format() {
        let sources = vec!["hash1".to_string()];
        let variants = HashMap::new();

        let key1 = CacheKey::compute(&sources, &variants, None, "json", "0.1.0");
        let key2 = CacheKey::compute(&sources, &variants, None, "yaml", "0.1.0");

        assert_ne!(key1.hash, key2.hash);
    }

    #[test]
    fn test_cache_key_changes_with_args() {
        let sources = vec!["hash1".to_string()];
        let variants = HashMap::new();

        let key1 = CacheKey::compute(&sources, &variants, None, "json", "0.1.0");
        let key2 = CacheKey::compute(&sources, &variants, Some("args_hash"), "json", "0.1.0");

        assert_ne!(key1.hash, key2.hash);
    }

    #[test]
    fn test_cache_miss_then_hit() {
        let dir = TempDir::new().unwrap();
        let cache = BuildCache::with_dir(dir.path().to_path_buf());

        let key = CacheKey::compute(
            &["source".to_string()],
            &HashMap::new(),
            None,
            "json",
            "0.1.0",
        );

        // Miss
        assert!(cache.get(&key).is_none());

        // Store
        let result =
            CachedResult::new(r#"{"key": "value"}"#.to_string(), "json", Some("test.hone"));
        cache.put(&key, &result).unwrap();

        // Hit
        let cached = cache.get(&key).unwrap();
        assert_eq!(cached.output, r#"{"key": "value"}"#);
        assert_eq!(cached.format, "json");
    }

    #[test]
    fn test_cache_invalidation() {
        let dir = TempDir::new().unwrap();
        let cache = BuildCache::with_dir(dir.path().to_path_buf());

        let key1 = CacheKey::compute(
            &["source_v1".to_string()],
            &HashMap::new(),
            None,
            "json",
            "0.1.0",
        );
        let key2 = CacheKey::compute(
            &["source_v2".to_string()],
            &HashMap::new(),
            None,
            "json",
            "0.1.0",
        );

        let result = CachedResult::new("output_v1".to_string(), "json", None);
        cache.put(&key1, &result).unwrap();

        // Different source => different key => miss
        assert!(cache.get(&key2).is_none());
        // Original key still hits
        assert!(cache.get(&key1).is_some());
    }

    #[test]
    fn test_cache_clean() {
        let dir = TempDir::new().unwrap();
        let cache = BuildCache::with_dir(dir.path().to_path_buf());

        // Store a few entries
        for i in 0..5 {
            let key = CacheKey::compute(
                &[format!("source_{}", i)],
                &HashMap::new(),
                None,
                "json",
                "0.1.0",
            );
            let result = CachedResult::new(format!("output_{}", i), "json", None);
            cache.put(&key, &result).unwrap();
        }

        let count = cache.clean().unwrap();
        assert_eq!(count, 5);

        // All entries should be gone
        let key = CacheKey::compute(
            &["source_0".to_string()],
            &HashMap::new(),
            None,
            "json",
            "0.1.0",
        );
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_hash_string() {
        let h1 = CacheKey::hash_string("hello");
        let h2 = CacheKey::hash_string("hello");
        let h3 = CacheKey::hash_string("world");

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64); // SHA256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(
            parse_duration("7d"),
            Some(std::time::Duration::from_secs(7 * 86400))
        );
        assert_eq!(
            parse_duration("24h"),
            Some(std::time::Duration::from_secs(24 * 3600))
        );
        assert_eq!(
            parse_duration("30m"),
            Some(std::time::Duration::from_secs(30 * 60))
        );
        assert_eq!(
            parse_duration("60s"),
            Some(std::time::Duration::from_secs(60))
        );
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("abc"), None);
    }

    #[test]
    fn test_cache_key_changes_with_multiple_sources() {
        let variants = HashMap::new();

        // Two-file build with source_a + source_b
        let key1 = CacheKey::compute(
            &["source_a".to_string(), "source_b".to_string()],
            &variants,
            None,
            "json",
            "0.1.0",
        );

        // Same root, but imported file changed (source_b -> source_c)
        let key2 = CacheKey::compute(
            &["source_a".to_string(), "source_c".to_string()],
            &variants,
            None,
            "json",
            "0.1.0",
        );

        // Only root changed
        let key3 = CacheKey::compute(
            &["source_x".to_string(), "source_b".to_string()],
            &variants,
            None,
            "json",
            "0.1.0",
        );

        assert_ne!(
            key1.hash, key2.hash,
            "changing an imported file must change the cache key"
        );
        assert_ne!(
            key1.hash, key3.hash,
            "changing the root file must change the cache key"
        );
        assert_ne!(
            key2.hash, key3.hash,
            "different changes must produce different keys"
        );
    }

    #[test]
    fn test_cache_key_includes_all_sources() {
        let variants = HashMap::new();

        // Single source
        let key_single =
            CacheKey::compute(&["source_a".to_string()], &variants, None, "json", "0.1.0");

        // Same source plus an additional imported file
        let key_multi = CacheKey::compute(
            &["source_a".to_string(), "source_b".to_string()],
            &variants,
            None,
            "json",
            "0.1.0",
        );

        assert_ne!(
            key_single.hash, key_multi.hash,
            "adding an imported file must change the cache key"
        );

        // Order matters: different topological order should produce different keys
        let key_reversed = CacheKey::compute(
            &["source_b".to_string(), "source_a".to_string()],
            &variants,
            None,
            "json",
            "0.1.0",
        );

        assert_ne!(
            key_multi.hash, key_reversed.hash,
            "different source ordering must produce different keys"
        );
    }
}
