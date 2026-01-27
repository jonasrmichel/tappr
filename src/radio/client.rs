use std::time::Duration;

use rand::seq::SliceRandom;
use reqwest::Client;
use tokio::time::sleep;
use tracing::{debug, instrument, warn};

use crate::app::StationInfo;
use crate::error::RadioError;

use super::types::*;

const BASE_URL: &str = "https://radio.garden/api";
const USER_AGENT: &str = concat!("tappr/", env!("CARGO_PKG_VERSION"));

/// Radio Garden API client
pub struct RadioGardenClient {
    client: Client,
    rate_limit_delay: Duration,
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
        }
    }

    /// Apply rate limiting delay
    async fn rate_limit(&self) {
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

    /// Get stations for a place
    #[instrument(skip(self))]
    pub async fn get_place_channels(&self, place_id: &str) -> Result<Vec<ChannelRef>, RadioError> {
        let url = format!("{}/ara/content/page/{}", BASE_URL, place_id);
        let response: ApiResponse<PlaceData> = self.get_json(&url).await?;

        // Flatten all channel sections
        let channels: Vec<ChannelRef> = response
            .data
            .content
            .into_iter()
            .flat_map(|section| section.items)
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
        let response: ApiResponse<SearchData> = self.get_json(&url).await?;

        let results: Vec<SearchSource> = response
            .data
            .hits
            .map(|h| h.hits.into_iter().map(|hit| hit.source).collect())
            .unwrap_or_default();

        debug!(count = results.len(), query, "Search complete");
        Ok(results)
    }

    /// Get a random station
    #[instrument(skip(self))]
    pub async fn random_station(&self) -> Result<StationInfo, RadioError> {
        let places = self.get_places().await?;

        // Filter to places with stations
        let valid_places: Vec<_> = places.into_iter().filter(|p| p.size > 0).collect();

        let place = valid_places
            .choose(&mut rand::thread_rng())
            .ok_or(RadioError::NoStationsFound)?;

        debug!(
            place = %place.title,
            country = %place.country,
            stations = place.size,
            "Selected random place"
        );

        let channels = self.get_place_channels(&place.id).await?;
        let channel_ref = channels
            .choose(&mut rand::thread_rng())
            .ok_or(RadioError::NoStationsFound)?;

        let channel_id = channel_ref.id().ok_or(RadioError::NoStationsFound)?;

        self.build_station_info(channel_id, Some(place.clone()))
            .await
    }

    /// Search and get first matching station
    #[instrument(skip(self))]
    pub async fn search_station(&self, query: &str) -> Result<StationInfo, RadioError> {
        let results = self.search(query).await?;

        // Find first channel result
        let channel = results
            .iter()
            .find(|r| r.is_channel())
            .ok_or(RadioError::NoStationsFound)?;

        let channel_id = channel.id().ok_or(RadioError::NoStationsFound)?;

        // If we have geo from search, use it; otherwise fetch details
        if let (Some(lat), Some(lon)) = (channel.latitude(), channel.longitude()) {
            let stream_url = self.get_stream_url(channel_id).await?;
            Ok(StationInfo {
                id: channel_id.to_string(),
                name: channel.title.clone(),
                country: String::new(), // Not available in search
                place_name: String::new(),
                latitude: lat,
                longitude: lon,
                stream_url: Some(stream_url),
            })
        } else {
            self.build_station_info(channel_id, None).await
        }
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
        let channel_ref = channels
            .choose(&mut rand::thread_rng())
            .ok_or(RadioError::NoStationsFound)?;

        let channel_id = channel_ref.id().ok_or(RadioError::NoStationsFound)?;

        self.build_station_info(channel_id, Some(place.clone()))
            .await
    }

    /// Build StationInfo from channel ID and optional place
    async fn build_station_info(
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
        })
    }
}
