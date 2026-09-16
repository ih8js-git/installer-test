use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use log::{debug, error, info};

use isahc::http::StatusCode;
use isahc::AsyncReadResponseExt;

use crate::CachedHttpClient;
use crate::Author;

use crate::preset::{Preset, PresetsContainer};

// Structure for a mod/component in the universal manifest
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ModComponent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub source: String,
    pub location: String,
    pub version: String,
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default = "default_false")]
    pub optional: bool,
    #[serde(default = "default_false")]
    pub default_enabled: bool,
    #[serde(default)]
    pub authors: Vec<Author>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub dependencies: Option<Vec<String>>,
    #[serde(default)]
    pub incompatibilities: Option<Vec<String>>,
    #[serde(default = "default_false")]
    pub ignore_update: bool,
}

// NEW: RemoteIncludeComponent structure
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RemoteIncludeComponent {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub source: String,
    pub location: String,
    #[serde(default)]
    pub path: Option<String>,
    pub version: String,
    #[serde(default = "default_false")]
    pub optional: bool,
    #[serde(default = "default_false")]
    pub default_enabled: bool,
    // NEW: Add can_reset field
    #[serde(default = "default_false")]
    pub can_reset: bool,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub authors: Vec<Author>,
    #[serde(default)]
    pub dependencies: Option<Vec<String>>,
    #[serde(default = "default_false")]
    pub ignore_update: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct UniversalManifest {
    pub manifest_version: i32,
    pub modpack_version: String,
    pub minecraft_version: String,
    pub name: String,
    pub subtitle: String,
    pub description: String,
    pub icon: bool,
    pub uuid: String,
    
    // Loader info
    pub loader: crate::Loader,
    
    // All available components
    pub mods: Vec<ModComponent>,
    #[serde(default)]
    pub shaderpacks: Vec<ModComponent>,
    #[serde(default)]
    pub resourcepacks: Vec<ModComponent>,
    
    // Include support
    #[serde(default)]
    pub include: Vec<IncludeComponent>,
    
    // NEW: Remote include support
    #[serde(default)]
    pub remote_include: Vec<RemoteIncludeComponent>,
    
    // Metadata
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub short_description: Option<String>,
    pub version: String,
    
    // Default settings
    #[serde(default)]
    pub max_mem: Option<i32>,
    #[serde(default)]
    pub min_mem: Option<i32>,
    #[serde(default)]
    pub java_args: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct IncludeComponent {
    pub location: String,
    #[serde(default = "default_empty_string")]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Option<Vec<Author>>,
    #[serde(default = "default_false")]
    pub optional: bool,
    #[serde(default = "default_false")]
    pub default_enabled: bool,
    #[serde(default = "default_false")]
    pub ignore_update: bool,
    // NEW: Add can_reset field
    #[serde(default = "default_false")]
    pub can_reset: bool,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub dependencies: Option<Vec<String>>,
}

fn default_empty_string() -> String {
    String::new()
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Clone)]
pub struct ManifestError {
    pub message: String,
    pub error_type: ManifestErrorType,
    pub file_name: String,
    pub raw_content: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManifestErrorType {
    NetworkError,
    SyntaxError,
    DeserializationError,
    ValidationError,
    UnknownError,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} in {}: {}", self.error_type, self.file_name, self.message)
    }
}

impl std::fmt::Display for ManifestErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestErrorType::NetworkError => write!(f, "Network Error"),
            ManifestErrorType::SyntaxError => write!(f, "Syntax Error"),
            ManifestErrorType::DeserializationError => write!(f, "Deserialization Error"),
            ManifestErrorType::ValidationError => write!(f, "Validation Error"),
            ManifestErrorType::UnknownError => write!(f, "Unknown Error"),
        }
    }
}

impl UniversalManifest {
    pub fn get_optional_includes(&self) -> Vec<&IncludeComponent> {
        self.include.iter()
            .filter(|include| include.optional && !include.id.is_empty())
            .collect()
    }
    
    // NEW: Get optional remote includes
    pub fn get_optional_remote_includes(&self) -> Vec<&RemoteIncludeComponent> {
        self.remote_include.iter()
            .filter(|remote| remote.optional)
            .collect()
    }
    
    pub fn get_all_optional_components(&self) -> Vec<ModComponent> {
        let mut components = Vec::new();
        
        // Add optional mods
        components.extend(
            self.mods.iter()
                .filter(|m| m.optional)
                .cloned()
        );
        
        // Add optional shaderpacks
        components.extend(
            self.shaderpacks.iter()
                .filter(|s| s.optional)
                .cloned()
        );
        
        // Add optional resourcepacks
        components.extend(
            self.resourcepacks.iter()
                .filter(|r| r.optional)
                .cloned()
        );
        
        // Convert optional includes to ModComponent format
        for include in self.get_optional_includes() {
            components.push(ModComponent {
                id: include.id.clone(),
                name: include.name.clone().unwrap_or_else(|| include.location.clone()),
                description: include.description.clone()
                    .or_else(|| Some(format!("Configuration file: {}", include.location))),
                source: "include".to_string(),
                location: include.location.clone(),
                version: "1.0".to_string(),
                path: None,
                optional: include.optional,
                default_enabled: include.default_enabled,
                authors: include.authors.clone().unwrap_or_default(),
                category: include.category.clone(), // Use the actual category
                dependencies: include.dependencies.clone(),
                incompatibilities: None,
                ignore_update: include.ignore_update,
            });
        }
        
        // NEW: Convert optional remote includes to ModComponent format
        for remote in self.get_optional_remote_includes() {
            components.push(ModComponent {
                id: remote.id.clone(),
                name: remote.name.clone().unwrap_or_else(|| remote.id.clone()),
                description: remote.description.clone(),
                source: "remote_include".to_string(),
                location: remote.location.clone(),
                version: remote.version.clone(),
                path: remote.path.as_ref().map(|p| PathBuf::from(p)),
                optional: remote.optional,
                default_enabled: remote.default_enabled,
                authors: remote.authors.clone(),
                category: remote.category.clone(), // Use actual category
                dependencies: remote.dependencies.clone(),
                incompatibilities: None,
                ignore_update: remote.ignore_update,
            });
        }
        
        components
    }
}

// Convert UniversalManifest to crate::Manifest format for compatibility
pub fn universal_to_manifest(universal: &UniversalManifest, enabled_features: Vec<String>) -> crate::Manifest {
    // Create features from components
    let mut features = vec![
        crate::Feature {
            id: "default".to_string(),
            name: "Core Components".to_string(),
            default: true,
            hidden: false,
            description: Some("Essential components for the modpack to function".to_string()),
        }
    ];
    
    // Add mods as features if they're optional
    for component in &universal.mods {
        if component.optional {
            features.push(crate::Feature {
                id: component.id.clone(),
                name: component.name.clone(),
                default: component.default_enabled,
                hidden: false,
                description: component.description.clone(),
            });
        }
    }
    
    // Similar for shaderpacks and resourcepacks
    for component in &universal.shaderpacks {
        if component.optional {
            features.push(crate::Feature {
                id: component.id.clone(),
                name: component.name.clone(),
                default: component.default_enabled,
                hidden: false,
                description: component.description.clone(),
            });
        }
    }
    
    for component in &universal.resourcepacks {
        if component.optional {
            features.push(crate::Feature {
                id: component.id.clone(),
                name: component.name.clone(),
                default: component.default_enabled,
                hidden: false,
                description: component.description.clone(),
            });
        }
    }
    
    // Add optional includes as features
    for include in &universal.include {
        if include.optional && !include.id.is_empty() {
            features.push(crate::Feature {
                id: include.id.clone(),
                name: include.name.clone().unwrap_or_else(|| include.location.clone()),
                default: include.default_enabled,
                hidden: false,
                description: include.description.clone()
                    .or_else(|| Some(format!("Include: {}", include.location))),
            });
        }
    }
    
    // NEW: Add optional remote includes as features
    for remote in &universal.remote_include {
        if remote.optional {
            features.push(crate::Feature {
                id: remote.id.clone(),
                name: remote.name.clone().unwrap_or_else(|| remote.id.clone()),
                default: remote.default_enabled,
                hidden: false,
                description: remote.description.clone(),
            });
        }
    }
    
    // Convert components to old format
    let mods = universal.mods.iter().map(|component| {
        crate::Mod {
            name: component.name.clone(),
            source: component.source.clone(),
            location: component.location.clone(),
            version: component.version.clone(),
            path: component.path.clone(),
            id: component.id.clone(),
            authors: component.authors.clone(),
            ignore_update: component.ignore_update,
        }
    }).collect();

    let shaderpacks = universal.shaderpacks.iter().map(|component| {
        crate::Shaderpack {
            name: component.name.clone(),
            source: component.source.clone(),
            location: component.location.clone(),
            version: component.version.clone(),
            path: component.path.clone(),
            id: component.id.clone(),
            authors: component.authors.clone(),
            ignore_update: component.ignore_update,
        }
    }).collect();

    let resourcepacks = universal.resourcepacks.iter().map(|component| {
        crate::Resourcepack {
            name: component.name.clone(),
            source: component.source.clone(),
            location: component.location.clone(),
            version: component.version.clone(),
            path: component.path.clone(),
            id: component.id.clone(),
            authors: component.authors.clone(),
            ignore_update: component.ignore_update,
        }
    }).collect();

let includes: Vec<crate::Include> = universal.include.iter().map(|inc| {
    crate::Include {
        location: inc.location.clone(),
        id: if inc.id.is_empty() { "default".to_string() } else { inc.id.clone() },
        name: inc.name.clone(),
        authors: inc.authors.clone(),
        optional: inc.optional,
        default_enabled: inc.default_enabled,
        ignore_update: inc.ignore_update,
        can_reset: inc.can_reset, // ADD this line
        description: inc.description.clone(), // ADD this line
        category: inc.category.clone(), // ADD this line
        dependencies: inc.dependencies.clone(), // ADD this line
    }
}).collect();
    
    // NEW: Convert remote includes to old RemoteInclude format
    let remote_include: Option<Vec<crate::RemoteInclude>> = if universal.remote_include.is_empty() {
        None
    } else {
        Some(universal.remote_include.iter().map(|remote| {
            crate::RemoteInclude {
                location: remote.location.clone(),
                path: remote.path.clone(),
                id: remote.id.clone(),
                version: remote.version.clone(),
                name: remote.name.clone(),
                authors: Some(remote.authors.clone()),
                // ADD THESE TWO LINES:
                optional: remote.optional,
                default_enabled: remote.default_enabled,
            }
        }).collect())
    };
    
    // Build the manifest
    crate::Manifest {
        manifest_version: universal.manifest_version,
        modpack_version: universal.modpack_version.clone(),
        name: universal.name.clone(),
        subtitle: universal.subtitle.clone(),
        tab_group: None,
        tab_title: None,
        tab_color: None,
        tab_background: None,
        tab_primary_font: None,
        tab_secondary_font: None,
        settings_background: None,
        popup_title: None,
        popup_contents: None,
        description: universal.description.clone(),
        icon: universal.icon,
        uuid: universal.uuid.clone(),
        loader: universal.loader.clone(),
        mods,
        shaderpacks,
        resourcepacks,
        remote_include, // NEW: Add the converted remote includes
        include: includes,
        features,
        trend: None,
        enabled_features,
        included_files: None,
        source: None,
        installer_path: None,
        max_mem: universal.max_mem,
        min_mem: universal.min_mem,
        java_args: universal.java_args.clone(),
        category: universal.category.clone(),
        is_new: None,
        short_description: universal.short_description.clone(),
    }
}

// Rest of the existing functions remain the same...
pub async fn load_universal_manifest(http_client: &CachedHttpClient, url: Option<&str>) -> Result<UniversalManifest, ManifestError> {
    let manifest_url = url.unwrap_or("https://raw.githubusercontent.com/Wynncraft-Overhaul/majestic-overhaul/master/universal.json");
    info!("Manifest URL being used: {}", manifest_url);
    debug!("Loading universal manifest from: {}", manifest_url);
    
    let mut retries = 0;
    const MAX_RETRIES: usize = 3;
    
    loop {
        match http_client.get_async(manifest_url).await {
            Ok(mut response) => {
                if response.status() != StatusCode::OK {
                    let status = response.status();
                    error!("Failed to fetch universal manifest: HTTP {}", status);
                    
                    if retries < MAX_RETRIES && (status.as_u16() >= 500 || status.as_u16() == 429) {
                        retries += 1;
                        debug!("Retrying request ({}/{})", retries, MAX_RETRIES);
                        tokio::time::sleep(tokio::time::Duration::from_millis(500 * retries as u64)).await;
                        continue;
                    }
                    
                    return Err(ManifestError {
                        message: format!("Failed to fetch universal manifest: HTTP {}", status),
                        error_type: ManifestErrorType::NetworkError,
                        file_name: "universal.json".to_string(),
                        raw_content: None,
                    });
                }
                
                match response.text().await {
                    Ok(manifest_json) => {
                        let raw_content = Some(manifest_json.clone());
                        
                        if let Err(json_err) = serde_json::from_str::<serde_json::Value>(&manifest_json) {
                            return Err(ManifestError {
                                message: format!("Invalid JSON syntax: {}", json_err),
                                error_type: ManifestErrorType::SyntaxError,
                                file_name: "universal.json".to_string(),
                                raw_content,
                            });
                        }
                        
                        match serde_json::from_str::<UniversalManifest>(&manifest_json) {
                            Ok(manifest) => {
                                debug!("Successfully loaded universal manifest for {}", manifest.name);
                                return Ok(manifest);
                            },
                            Err(e) => {
                                error!("Failed to parse universal manifest JSON: {}", e);
                                
                                return Err(ManifestError {
                                    message: format!("Failed to parse universal manifest: {}", e),
                                    error_type: ManifestErrorType::DeserializationError,
                                    file_name: "universal.json".to_string(),
                                    raw_content,
                                });
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to read universal manifest response: {}", e);
                        
                        if retries < MAX_RETRIES {
                            retries += 1;
                            debug!("Retrying request ({}/{})", retries, MAX_RETRIES);
                            tokio::time::sleep(tokio::time::Duration::from_millis(500 * retries as u64)).await;
                            continue;
                        }
                        
                        return Err(ManifestError {
                            message: format!("Failed to read universal manifest: {}", e),
                            error_type: ManifestErrorType::NetworkError,
                            file_name: "universal.json".to_string(),
                            raw_content: None,
                        });
                    }
                }
            },
            Err(e) => {
                error!("Failed to fetch universal manifest: {}", e);
                
                if retries < MAX_RETRIES {
                    retries += 1;
                    debug!("Retrying request ({}/{})", retries, MAX_RETRIES);
                    tokio::time::sleep(tokio::time::Duration::from_millis(500 * retries as u64)).await;
                    continue;
                }
                
                return Err(ManifestError {
                    message: format!("Failed to fetch universal manifest: {}", e),
                    error_type: ManifestErrorType::NetworkError,
                    file_name: "universal.json".to_string(),
                    raw_content: None,
                });
            }
        }
    }
}
