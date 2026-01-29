use std::time::Duration;

use rand::seq::SliceRandom;
use reqwest::Client;
use tokio::time::sleep;
use tracing::{debug, instrument, warn};

use crate::app::StationInfo;
use crate::error::RadioError;

use super::types::*;

const BASE_URL: &str = "https://radio.garden/api";
// Use browser-like User-Agent to avoid Cloudflare blocking
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Radio Garden API client
pub struct RadioGardenClient {
    client: Client,
    rate_limit_delay: Duration,
    /// Skip rate limiting for first N requests (for fast startup)
    skip_rate_limit_count: std::sync::atomic::AtomicU32,
}

impl RadioGardenClient {
    pub fn new(rate_limit_ms: u64) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            rate_limit_delay: Duration::from_millis(rate_limit_ms),
            // Skip rate limiting for first 5 requests to speed up startup
            skip_rate_limit_count: std::sync::atomic::AtomicU32::new(5),
        }
    }

    /// Apply rate limiting delay (skipped for initial requests)
    async fn rate_limit(&self) {
        let remaining = self.skip_rate_limit_count.load(std::sync::atomic::Ordering::Relaxed);
        if remaining > 0 {
            self.skip_rate_limit_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            return; // Skip rate limiting for startup
        }
        sleep(self.rate_limit_delay).await;
    }

    /// Make a GET request and parse JSON response
    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, RadioError> {
        self.rate_limit().await;
        debug!(url, "Fetching");

        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(RadioError::HttpStatus(response.status()));
        }

        let json = response.json().await?;
        Ok(json)
    }

    /// Fetch all places (cities with stations)
    #[instrument(skip(self))]
    pub async fn get_places(&self) -> Result<Vec<Place>, RadioError> {
        let url = format!("{}/ara/content/places", BASE_URL);
        let response: ApiResponse<PlacesData> = self.get_json(&url).await?;
        debug!(count = response.data.list.len(), "Fetched places");
        Ok(response.data.list)
    }

    /// Get stations for a place (public for RadioService)
    #[instrument(skip(self))]
    pub async fn get_place_channels(&self, place_id: &str) -> Result<Vec<ChannelRef>, RadioError> {
        let url = format!("{}/ara/content/page/{}", BASE_URL, place_id);
        let response: ApiResponse<PlaceData> = self.get_json(&url).await?;

        // Flatten all channel sections and unwrap the page field
        let channels: Vec<ChannelRef> = response
            .data
            .content
            .into_iter()
            .flat_map(|section| section.items.into_iter().map(|item| item.page))
            .collect();

        debug!(count = channels.len(), place_id, "Fetched place channels");
        Ok(channels)
    }

    /// Get channel details
    #[instrument(skip(self))]
    pub async fn get_channel(&self, channel_id: &str) -> Result<ChannelData, RadioError> {
        let url = format!("{}/ara/content/channel/{}", BASE_URL, channel_id);
        let response: ApiResponse<ChannelData> = self.get_json(&url).await?;
        debug!(channel_id, title = %response.data.title, "Fetched channel");
        Ok(response.data)
    }

    /// Get stream URL for a channel (follows redirects)
    #[instrument(skip(self))]
    pub async fn get_stream_url(&self, channel_id: &str) -> Result<String, RadioError> {
        self.rate_limit().await;

        let url = format!("{}/ara/content/listen/{}/channel.mp3", BASE_URL, channel_id);
        debug!(url, "Resolving stream URL");

        // Use GET with redirect following to get final URL
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(RadioError::HttpStatus(response.status()));
        }

        let final_url = response.url().to_string();
        debug!(stream_url = %final_url, "Resolved stream URL");
        Ok(final_url)
    }

    /// Search for stations
    #[instrument(skip(self))]
    pub async fn search(&self, query: &str) -> Result<Vec<SearchSource>, RadioError> {
        let url = format!("{}/search?q={}", BASE_URL, urlencoding::encode(query));
        // Search endpoint has different response structure (not wrapped in ApiResponse)
        let response: SearchResponse = self.get_json(&url).await?;

        let results: Vec<SearchSource> = response
            .hits
            .map(|h| h.hits.into_iter().map(|hit| hit.source).collect())
            .unwrap_or_default();

        debug!(count = results.len(), query, "Search complete");
        Ok(results)
    }

    /// Get a random station (now handled by RadioService with caching)
    #[allow(dead_code)]
    #[instrument(skip(self))]
    pub async fn random_station(&self) -> Result<StationInfo, RadioError> {
        let places = self.get_places().await?;
        debug!(total_places = places.len(), "Fetched places list");

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
            place_id = %place.id,
            "Selected random place"
        );

        let channels = self.get_place_channels(&place.id).await?;
        debug!(channels_count = channels.len(), "Fetched channels for place");

        // Filter to channels with valid IDs (those starting with /listen/)
        let valid_channels: Vec<_> = channels.iter().filter(|c| c.id().is_some()).collect();
        debug!(valid_channels = valid_channels.len(), "Channels with valid IDs");

        if valid_channels.is_empty() {
            if let Some(first) = channels.first() {
                debug!(first_channel_url = %first.url, "First channel URL for debugging");
            }
        }

        let channel_ref = valid_channels
            .choose(&mut rand::thread_rng())
            .ok_or(RadioError::NoStationsFound)?;

        let channel_id = channel_ref.id().ok_or(RadioError::NoStationsFound)?;
        debug!(channel_id, "Selected channel");

        self.build_station_info(channel_id, Some(place.clone()))
            .await
    }

    /// Search and get first matching station
    #[instrument(skip(self))]
    pub async fn search_station(&self, query: &str) -> Result<StationInfo, RadioError> {
        let results = self.search(query).await?;

        // Find first channel result with valid page data
        let channel = results
            .iter()
            .find(|r| r.is_channel() && r.page().is_some())
            .ok_or(RadioError::NoStationsFound)?;

        let channel_id = channel.id().ok_or(RadioError::NoStationsFound)?;

        // Fetch full channel details to get accurate location
        self.build_station_info(channel_id, None).await
    }

    /// Get station by region/country
    #[instrument(skip(self))]
    pub async fn station_by_region(&self, region: &str) -> Result<StationInfo, RadioError> {
        let places = self.get_places().await?;

        // Filter places by country name (case-insensitive contains)
        let region_lower = region.to_lowercase();
        let matching: Vec<_> = places
            .into_iter()
            .filter(|p| p.country.to_lowercase().contains(&region_lower) && p.size > 0)
            .collect();

        if matching.is_empty() {
            warn!(region, "No places found for region");
            return Err(RadioError::NoStationsFound);
        }

        let place = matching
            .choose(&mut rand::thread_rng())
            .ok_or(RadioError::NoStationsFound)?;

        debug!(
            place = %place.title,
            country = %place.country,
            "Selected place in region"
        );

        let channels = self.get_place_channels(&place.id).await?;

        // Filter to valid channels, preferring FM over AM
        let valid_channels: Vec<_> = channels.iter().filter(|c| c.id().is_some()).collect();
        let fm_channels: Vec<_> = valid_channels.iter().filter(|c| !c.is_am()).copied().collect();

        let channel_ref = if fm_channels.is_empty() {
            valid_channels.choose(&mut rand::thread_rng()).ok_or(RadioError::NoStationsFound)?
        } else {
            fm_channels.choose(&mut rand::thread_rng()).ok_or(RadioError::NoStationsFound)?
        };

        let channel_id = channel_ref.id().ok_or(RadioError::NoStationsFound)?;

        self.build_station_info(channel_id, Some(place.clone()))
            .await
    }

    /// Build StationInfo from channel ID and optional place (public for RadioService)
    pub async fn build_station_info(
        &self,
        channel_id: &str,
        place: Option<Place>,
    ) -> Result<StationInfo, RadioError> {
        let channel = self.get_channel(channel_id).await?;
        let stream_url = self.get_stream_url(channel_id).await?;

        // If we have place info, use its coordinates; otherwise we need to fetch place
        let (latitude, longitude, place_name) = if let Some(p) = place {
            (p.latitude(), p.longitude(), p.title)
        } else {
            // Fetch place to get coordinates
            let channels = self.get_place_channels(&channel.place.id).await;
            if channels.is_ok() {
                // Get place from places list
                let places = self.get_places().await?;
                if let Some(p) = places.iter().find(|p| p.id == channel.place.id) {
                    (p.latitude(), p.longitude(), p.title.clone())
                } else {
                    (0.0, 0.0, channel.place.title.clone())
                }
            } else {
                (0.0, 0.0, channel.place.title.clone())
            }
        };

        Ok(StationInfo {
            id: channel.id,
            name: channel.title,
            country: channel.country.title,
            place_name,
            latitude,
            longitude,
            stream_url: Some(stream_url),
            website: channel.website,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_random_station() {
        let client = RadioGardenClient::new(100);
        
        println!("Testing get_places...");
        let places = client.get_places().await.expect("Failed to get places");
        println!("Got {} places", places.len());
        
        let valid: Vec<_> = places.into_iter().filter(|p| p.size > 0).collect();
        println!("Valid places with stations: {}", valid.len());
        
        let place = valid.first().expect("No valid places");
        println!("First place: {} ({}) - {} stations", place.title, place.country, place.size);
        
        println!("Testing get_place_channels for {}...", place.id);
        let channels = client.get_place_channels(&place.id).await.expect("Failed to get channels");
        println!("Got {} channels", channels.len());
        
        for (i, ch) in channels.iter().take(5).enumerate() {
            println!("  Channel {}: url={}, id={:?}", i, ch.url, ch.id());
        }
        
        let valid_channels: Vec<_> = channels.iter().filter(|c| c.id().is_some()).collect();
        println!("Valid channels (with /listen/ URL): {}", valid_channels.len());
        
        assert!(valid_channels.len() > 0, "No valid channels found!");
    }
}
