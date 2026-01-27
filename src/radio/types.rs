use serde::{Deserialize, Serialize};

/// Generic API response wrapper from Radio Garden
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T> {
    pub api_version: u32,
    #[allow(dead_code)]
    pub version: Option<String>,
    pub data: T,
}

/// Places list response data
#[derive(Debug, Deserialize)]
pub struct PlacesData {
    pub list: Vec<Place>,
}

/// A place (city) with radio stations
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Place {
    pub id: String,
    pub title: String,
    pub country: String,
    #[allow(dead_code)]
    pub url: String,
    pub size: u32, // number of stations
    #[allow(dead_code)]
    pub boost: bool,
    pub geo: [f64; 2], // [lon, lat] - note: Radio Garden uses [longitude, latitude]
}

impl Place {
    pub fn latitude(&self) -> f64 {
        self.geo[1] // geo is [lon, lat]
    }

    pub fn longitude(&self) -> f64 {
        self.geo[0] // geo is [lon, lat]
    }
}

/// Channels list response data
#[derive(Debug, Deserialize)]
pub struct ChannelsData {
    #[allow(dead_code)]
    pub title: Option<String>,
    pub content: Vec<ChannelSection>,
}

/// Section containing channel references
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSection {
    #[allow(dead_code)]
    pub items_type: Option<String>,
    pub items: Vec<ChannelRef>,
}

/// Item wrapper in place listing (contains a page object)
#[derive(Debug, Clone, Deserialize)]
pub struct ChannelItem {
    pub page: ChannelRef,
}

/// Channel reference in place listing
#[derive(Debug, Clone, Deserialize)]
pub struct ChannelRef {
    pub url: String,
    pub title: String,
}

impl ChannelRef {
    /// Extract channel ID from url (e.g., "/listen/3fm/UsxY2mdY" -> "UsxY2mdY")
    pub fn id(&self) -> Option<&str> {
        // URL format: /listen/{name}/{id}
        self.url.strip_prefix("/listen/").and_then(|s| s.rsplit('/').next())
    }
}

/// Full channel details response data
#[derive(Debug, Deserialize)]
pub struct ChannelData {
    pub id: String,
    pub title: String,
    #[allow(dead_code)]
    pub url: String,
    #[allow(dead_code)]
    pub website: Option<String>,
    #[allow(dead_code)]
    pub secure: bool,
    pub place: PlaceRef,
    pub country: CountryRef,
}

/// Place reference in channel details
#[derive(Debug, Clone, Deserialize)]
pub struct PlaceRef {
    pub id: String,
    pub title: String,
}

/// Country reference in channel details
#[derive(Debug, Clone, Deserialize)]
pub struct CountryRef {
    #[allow(dead_code)]
    pub id: String,
    pub title: String,
}

/// Search response (different structure from other API responses)
#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    #[allow(dead_code)]
    pub took: Option<u32>,
    #[allow(dead_code)]
    pub query: Option<String>,
    pub hits: Option<SearchHits>,
}

/// Search hits container
#[derive(Debug, Deserialize)]
pub struct SearchHits {
    pub hits: Vec<SearchHit>,
}

/// Individual search hit
#[derive(Debug, Deserialize)]
pub struct SearchHit {
    #[serde(rename = "_source")]
    pub source: SearchSource,
}

/// Search result source data
#[derive(Debug, Deserialize)]
pub struct SearchSource {
    #[allow(dead_code)]
    pub code: Option<String>,
    /// Nested page data with actual channel/place info
    pub page: Option<SearchPage>,
    #[serde(rename = "type")]
    pub result_type: String,
}

/// Nested page data in search results
#[derive(Debug, Deserialize)]
pub struct SearchPage {
    pub url: String,
    pub title: String,
    pub place: Option<PlaceRef>,
    pub country: Option<CountryRef>,
    #[allow(dead_code)]
    pub subtitle: Option<String>,
}

impl SearchSource {
    /// Get the page data
    pub fn page(&self) -> Option<&SearchPage> {
        self.page.as_ref()
    }

    /// Extract ID from URL path
    pub fn id(&self) -> Option<&str> {
        self.page.as_ref().and_then(|p| p.url.rsplit('/').next())
    }

    /// Get title from nested page
    pub fn title(&self) -> Option<&str> {
        self.page.as_ref().map(|p| p.title.as_str())
    }

    /// Check if this is a channel result
    pub fn is_channel(&self) -> bool {
        self.result_type == "channel"
    }

    /// Check if this is a place result
    pub fn is_place(&self) -> bool {
        self.result_type == "place"
    }
}

/// Place details response data
#[derive(Debug, Deserialize)]
pub struct PlaceData {
    #[allow(dead_code)]
    pub title: String,
    #[allow(dead_code)]
    pub url: String,
    #[allow(dead_code)]
    pub subtitle: Option<String>,
    pub content: Vec<PlaceContent>,
}

/// Content section in place details
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceContent {
    #[allow(dead_code)]
    pub items_type: Option<String>,
    pub items: Vec<ChannelItem>,
}
