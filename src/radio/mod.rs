mod cache;
mod client;
mod types;

pub use cache::RadioCache;
pub use client::RadioGardenClient;
pub use types::Place;

use std::sync::Arc;

use tracing::{debug, instrument};

use crate::app::StationInfo;
use crate::error::RadioError;

/// Combined Radio Garden service with caching
pub struct RadioService {
    client: RadioGardenClient,
    #[allow(dead_code)]
    cache: Arc<RadioCache>,
}

impl RadioService {
    pub fn new(rate_limit_ms: u64, cache_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            client: RadioGardenClient::new(rate_limit_ms),
            cache: Arc::new(RadioCache::new(cache_dir)),
        }
    }

    /// Get all places (with caching)
    #[allow(dead_code)]
    #[instrument(skip(self))]
    pub async fn get_places(&self) -> Result<Vec<Place>, RadioError> {
        // Check cache first
        if let Some(places) = self.cache.get_places() {
            return Ok(places);
        }

        // Fetch from API
        let places = self.client.get_places().await?;
        self.cache.set_places(places.clone());
        Ok(places)
    }

    /// Get a random station
    #[instrument(skip(self))]
    pub async fn random_station(&self) -> Result<StationInfo, RadioError> {
        self.client.random_station().await
    }

    /// Search and get first matching station
    #[instrument(skip(self))]
    pub async fn search_station(&self, query: &str) -> Result<StationInfo, RadioError> {
        self.client.search_station(query).await
    }

    /// Get a station from a specific region
    #[instrument(skip(self))]
    pub async fn station_by_region(&self, region: &str) -> Result<StationInfo, RadioError> {
        self.client.station_by_region(region).await
    }

    /// Get the next station based on current selection mode
    #[instrument(skip(self))]
    pub async fn next_station(
        &self,
        search: Option<&str>,
        region: Option<&str>,
    ) -> Result<StationInfo, RadioError> {
        if let Some(query) = search {
            debug!(query, "Getting station by search");
            self.search_station(query).await
        } else if let Some(region) = region {
            debug!(region, "Getting station by region");
            self.station_by_region(region).await
        } else {
            debug!("Getting random station");
            self.random_station().await
        }
    }
}
