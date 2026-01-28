mod cache;
mod client;
mod types;

pub use cache::RadioCache;
pub use client::RadioGardenClient;
pub use types::Place;

use std::sync::Arc;

use rand::seq::SliceRandom;
use tracing::{debug, info, instrument};

use crate::app::StationInfo;
use crate::error::RadioError;

/// Combined Radio Garden service with caching
pub struct RadioService {
    client: RadioGardenClient,
    cache: Arc<RadioCache>,
}

impl RadioService {
    #[allow(dead_code)]
    pub fn new(rate_limit_ms: u64, cache_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            client: RadioGardenClient::new(rate_limit_ms),
            cache: Arc::new(RadioCache::new(cache_dir)),
        }
    }

    /// Create with a shared cache (for sharing pre-warmed cache between workers)
    pub fn with_shared_cache(rate_limit_ms: u64, cache: Arc<RadioCache>) -> Self {
        Self {
            client: RadioGardenClient::new(rate_limit_ms),
            cache,
        }
    }

    /// Get the cache (for sharing with other services)
    #[allow(dead_code)]
    pub fn cache(&self) -> Arc<RadioCache> {
        Arc::clone(&self.cache)
    }

    /// Pre-fetch places to warm up cache (call at startup)
    #[instrument(skip(self))]
    pub async fn warm_up(&self) -> Result<(), RadioError> {
        if self.cache.get_places().is_some() {
            debug!("Places already cached");
            return Ok(());
        }

        info!("Pre-fetching places for faster startup...");
        let places = self.client.get_places().await?;
        self.cache.set_places(places.clone());
        info!(count = places.len(), "Places cached");
        Ok(())
    }

    /// Get all places (with caching)
    #[instrument(skip(self))]
    pub async fn get_places(&self) -> Result<Vec<Place>, RadioError> {
        // Check cache first
        if let Some(places) = self.cache.get_places() {
            debug!(count = places.len(), "Using cached places");
            return Ok(places);
        }

        // Fetch from API
        let places = self.client.get_places().await?;
        self.cache.set_places(places.clone());
        Ok(places)
    }

    /// Get a random station (uses cached places if available)
    #[instrument(skip(self))]
    pub async fn random_station(&self) -> Result<StationInfo, RadioError> {
        // Use cached places if available for faster startup
        let places = self.get_places().await?;

        // Filter to places with stations
        let valid_places: Vec<_> = places.into_iter().filter(|p| p.size > 0).collect();
        debug!(valid_places = valid_places.len(), "Filtered valid places");

        let place = valid_places
            .choose(&mut rand::thread_rng())
            .ok_or(RadioError::NoStationsFound)?;

        debug!(
            place = %place.title,
            country = %place.country,
            stations = place.size,
            "Selected random place"
        );

        let channels = self.client.get_place_channels(&place.id).await?;

        // Filter to channels with valid IDs
        let valid_channels: Vec<_> = channels.iter().filter(|c| c.id().is_some()).collect();

        let channel_ref = valid_channels
            .choose(&mut rand::thread_rng())
            .ok_or(RadioError::NoStationsFound)?;

        let channel_id = channel_ref.id().ok_or(RadioError::NoStationsFound)?;

        self.client
            .build_station_info(channel_id, Some(place.clone()))
            .await
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
