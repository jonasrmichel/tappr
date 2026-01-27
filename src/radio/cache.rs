use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use parking_lot::RwLock;
use tracing::{debug, warn};

use super::types::Place;

/// Time-to-live for cached places (1 hour)
const PLACES_TTL: Duration = Duration::from_secs(3600);

/// Cached places with timestamp
struct CachedPlaces {
    places: Vec<Place>,
    timestamp: SystemTime,
}

/// Cache for Radio Garden data
pub struct RadioCache {
    cache_dir: PathBuf,
    places: RwLock<Option<CachedPlaces>>,
}

impl RadioCache {
    pub fn new(cache_dir: Option<PathBuf>) -> Self {
        let cache_dir = cache_dir.unwrap_or_else(|| {
            dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("tappr")
        });

        // Create cache directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            warn!(path = ?cache_dir, error = %e, "Failed to create cache directory");
        }

        debug!(path = ?cache_dir, "Initialized cache");

        Self {
            cache_dir,
            places: RwLock::new(None),
        }
    }

    /// Get cache directory path
    pub fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    /// Get cached places if still valid
    pub fn get_places(&self) -> Option<Vec<Place>> {
        let cache = self.places.read();
        if let Some(cached) = cache.as_ref() {
            if cached.timestamp.elapsed().unwrap_or(PLACES_TTL) < PLACES_TTL {
                debug!("Using cached places");
                return Some(cached.places.clone());
            }
        }
        None
    }

    /// Store places in cache
    pub fn set_places(&self, places: Vec<Place>) {
        debug!(count = places.len(), "Caching places");
        let mut cache = self.places.write();
        *cache = Some(CachedPlaces {
            places,
            timestamp: SystemTime::now(),
        });
    }

    /// Clear all caches
    #[allow(dead_code)]
    pub fn clear(&self) {
        debug!("Clearing cache");
        *self.places.write() = None;
    }

    /// Get path for a cache file
    #[allow(dead_code)]
    pub fn cache_file(&self, name: &str) -> PathBuf {
        self.cache_dir.join(name)
    }
}
