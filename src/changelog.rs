use serde::{Deserialize, Serialize};
use log::{debug, warn};
use isahc::http::StatusCode;
use isahc::AsyncReadResponseExt;

/// Entry in the changelog
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ChangelogEntry {
    pub title: String,
    pub contents: String, 
    pub date: Option<String>,
    pub version: Option<String>,
    pub importance: Option<String>,  // "major", "minor", "bugfix"
}

/// Statistics for the home page
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct HomePageStats {
    pub stat1_value: String,
    pub stat1_label: String,
    pub stat2_value: String,
    pub stat2_label: String,
    pub stat3_value: String,
    pub stat3_label: String,
}

/// Footer button configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FooterButton {
    pub text: String,
    pub link: String,
}

/// Home page configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[derive(Default)]
pub struct HomePageConfig {
    pub stats: HomePageStats,
    pub footer_button: FooterButton,
}

/// Complete changelog structure with home page config
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Changelog {
    pub entries: Vec<ChangelogEntry>,
    #[serde(default)]
    pub homepage_config: Option<HomePageConfig>,
}

// Default implementations for backwards compatibility
impl Default for HomePageStats {
    fn default() -> Self {
        Self {
            stat1_value: "90+".to_string(),
            stat1_label: "MODS".to_string(),
            stat2_value: "200+".to_string(),
            stat2_label: "FPS".to_string(),
            stat3_value: "20K+".to_string(),
            stat3_label: "DOWNLOADS".to_string(),
        }
    }
}

impl Default for FooterButton {
    fn default() -> Self {
        Self {
            text: "JOIN OUR DISCORD".to_string(),
            link: "https://discord.gg/olinus-corner-778965021656743966".to_string(),
        }
    }
}


/// Load the changelog from the repository
pub async fn fetch_changelog(
    _modpack_source: &str, 
    http_client: &crate::CachedHttpClient
) -> Result<Changelog, String> {
    
    let base_url = "https://raw.githubusercontent.com/Wynncraft-Overhaul/majestic-overhaul/master/";
    let changelog_url = format!("{}changelog.json", base_url);
    
    let mut changelog_resp = match http_client.get_async(changelog_url.clone()).await {
        Ok(val) => val,
        Err(e) => {
            warn!("Failed to fetch changelog: {}", e);
            return Err(format!("Failed to fetch changelog: {}", e));
        }
    };
    
    if changelog_resp.status() != StatusCode::OK {
        warn!("Changelog returned non-200 status: {}", changelog_resp.status());
        return Err(format!("Changelog returned status: {}", changelog_resp.status()));
    }
    
    let changelog_text = match changelog_resp.text().await {
        Ok(text) => text,
        Err(e) => {
            warn!("Failed to read changelog response: {}", e);
            return Err(format!("Failed to read changelog response: {}", e));
        }
    };
    
    match serde_json::from_str::<Changelog>(&changelog_text) {
        Ok(changelog) => {
            debug!("Successfully parsed changelog with {} entries", changelog.entries.len());
            Ok(changelog)
        },
        Err(e) => {
            warn!("Failed to parse changelog: {}", e);
            Err(format!("Failed to parse changelog: {}", e))
        }
    }
}
