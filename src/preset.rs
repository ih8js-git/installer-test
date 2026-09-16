use serde::{Deserialize, Serialize};
use log::{debug, error};

use isahc::AsyncReadResponseExt;
use crate::CachedHttpClient;

use crate::universal::ManifestError;
use crate::universal::ManifestErrorType;
use isahc::http::StatusCode;

// Structure defining a single preset configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub icon: Option<String>,
    
    // NEW: Add preset version support
    pub preset_version: Option<String>,
    
    // Enabled component IDs
    pub enabled_features: Vec<String>,
    
    // Recommended settings
    pub recommended_memory: Option<i32>,
    pub recommended_java_args: Option<String>,
    
    // Whether this preset is featured/trending
    pub trending: Option<bool>,
    
    // Category for organization purposes
    pub category: Option<String>,
    
    // Visual customization
    pub background: Option<String>,
    pub color: Option<String>,
}

// Container for all presets to parse from JSON
#[derive(Debug, Deserialize, Serialize)]
pub struct PresetsContainer {
    pub version: String,
    pub last_updated: String,
    pub presets: Vec<Preset>,
}

// NEW: Helper function to check if a preset has been updated
pub fn check_preset_version_update(preset: &Preset, local_version: Option<&str>) -> bool {
    match (&preset.preset_version, local_version) {
        (Some(remote_version), Some(local_version)) => {
            // Simple string comparison - you could implement semantic versioning here
            remote_version != local_version
        },
        (Some(_), None) => true, // New preset version available
        _ => false, // No version info available
    }
}

// NEW: Check if modpack has updates by comparing versions
pub fn check_modpack_update(universal_version: &str, local_version: Option<&str>) -> bool {
    match local_version {
        Some(local) => universal_version != local,
        None => true, // First install
    }
}

// Default URL for presets - UPDATED to new repository
const DEFAULT_PRESETS_URL: &str = "https://raw.githubusercontent.com/Wynncraft-Overhaul/majestic-overhaul/master/presets.json";

impl Preset {
    // Apply this preset to an installation, returning the list of enabled features
    pub fn apply_to_installation(&self, installation: &mut crate::installation::Installation) {
        debug!("Applying preset '{}' to installation '{}'", self.name, installation.name);
        
        // Update enabled features
        installation.enabled_features = self.enabled_features.clone();
        
        // Optionally update recommended settings
        if let Some(memory) = self.recommended_memory {
            installation.memory_allocation = memory;
            debug!("Updated memory allocation to {}", memory);
        }
        
        if let Some(java_args) = &self.recommended_java_args {
            installation.java_args = java_args.clone();
            debug!("Updated Java args to '{}'", java_args);
        }
        
        // Mark as modified and requiring update
        installation.modified = true;
    }
}

// Function to load presets from a URL - UPDATED to use new repository
pub async fn load_presets(http_client: &CachedHttpClient, url: Option<&str>) -> Result<Vec<Preset>, ManifestError> {
    let presets_url = url.unwrap_or("https://raw.githubusercontent.com/Wynncraft-Overhaul/majestic-overhaul/master/presets.json");
    debug!("Loading presets from: {}", presets_url);
    
    // Add retry logic for more reliability
    let mut retries = 0;
    const MAX_RETRIES: usize = 3;
    
    loop {
        match http_client.get_async(presets_url).await {
            Ok(mut response) => {
                let status = response.status();
                debug!("Presets HTTP status: {}", status);
                
                if status != StatusCode::OK {
                    error!("Failed to fetch presets: HTTP {}", status);
                    
                    if retries < MAX_RETRIES && (status.as_u16() >= 500 || status.as_u16() == 429) {
                        retries += 1;
                        debug!("Retrying request ({}/{})", retries, MAX_RETRIES);
                        tokio::time::sleep(tokio::time::Duration::from_millis(500 * retries as u64)).await;
                        continue;
                    }
                    
                    return Err(ManifestError {
                        message: format!("Failed to fetch presets: HTTP {}", status),
                        error_type: ManifestErrorType::NetworkError,
                        file_name: "presets.json".to_string(),
                        raw_content: None,
                    });
                }
                
                // Get text as String
                match response.text().await {
                    Ok(presets_json) => {
                        debug!("Received presets JSON of length: {}", presets_json.len());
                        
                        // Store the raw JSON for debugging
                        let raw_content = Some(presets_json.clone());
                        
                        // Try parsing as regular JSON first to catch syntax errors
                        if let Err(json_err) = serde_json::from_str::<serde_json::Value>(&presets_json) {
                            return Err(ManifestError {
                                message: format!("Invalid JSON syntax: {}", json_err),
                                error_type: ManifestErrorType::SyntaxError,
                                file_name: "presets.json".to_string(),
                                raw_content,
                            });
                        }
                        
                        // Parse the presets container
                        match serde_json::from_str::<PresetsContainer>(&presets_json) {
                            Ok(container) => {
                                debug!("Successfully loaded {} presets (version: {})", 
                                      container.presets.len(), container.version);
                                
                                // Log preset information for debugging
                                for preset in &container.presets {
                                    debug!("Loaded preset: {} (ID: {}, Version: {:?})", 
                                           preset.name, preset.id, preset.preset_version);
                                }
                                
                                return Ok(container.presets);
                            },
                            Err(e) => {
                                error!("Failed to parse presets JSON: {}", e);
                                
                                return Err(ManifestError {
                                    message: format!("Failed to parse presets: {}", e),
                                    error_type: ManifestErrorType::DeserializationError,
                                    file_name: "presets.json".to_string(),
                                    raw_content,
                                });
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to read presets response: {}", e);
                        
                        if retries < MAX_RETRIES {
                            retries += 1;
                            debug!("Retrying request ({}/{})", retries, MAX_RETRIES);
                            tokio::time::sleep(tokio::time::Duration::from_millis(500 * retries as u64)).await;
                            continue;
                        }
                        
                        return Err(ManifestError {
                            message: format!("Failed to read presets response: {}", e),
                            error_type: ManifestErrorType::NetworkError,
                            file_name: "presets.json".to_string(),
                            raw_content: None,
                        });
                    }
                }
            },
            Err(e) => {
                error!("Failed to fetch presets: {}", e);
                
                if retries < MAX_RETRIES {
                    retries += 1;
                    debug!("Retrying request ({}/{})", retries, MAX_RETRIES);
                    tokio::time::sleep(tokio::time::Duration::from_millis(500 * retries as u64)).await;
                    continue;
                }
                
                return Err(ManifestError {
                    message: format!("Failed to fetch presets: {}", e),
                    error_type: ManifestErrorType::NetworkError,
                    file_name: "presets.json".to_string(),
                    raw_content: None,
                });
            }
        }
    }
}

// Find a preset by ID
pub fn find_preset_by_id(presets: &[Preset], id: &str) -> Option<Preset> {
    debug!("Looking for preset with ID: {}", id);
    
    let result = presets.iter()
        .find(|preset| preset.id == id)
        .cloned();
        
    match &result {
        Some(preset) => debug!("Found preset: {}", preset.name),
        None => debug!("No preset found with ID: {}", id),
    }
    
    result
}
