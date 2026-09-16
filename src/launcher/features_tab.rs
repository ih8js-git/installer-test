use dioxus::prelude::*;
use crate::universal::{ModComponent, UniversalManifest};
use crate::preset::{Preset, find_preset_by_id};
use log::debug;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

// Global session state storage
static SESSION_STATE: Lazy<Mutex<HashMap<String, SessionInstallationState>>> = 
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Debug)]
struct SessionInstallationState {
    selected_preset_id: Option<String>,
    enabled_features: Vec<String>,
    last_modified: std::time::Instant,
}

impl SessionInstallationState {
    fn new(preset_id: Option<String>, features: Vec<String>) -> Self {
        Self {
            selected_preset_id: preset_id,
            enabled_features: features,
            last_modified: std::time::Instant::now(),
        }
    }
    
    fn update(&mut self, preset_id: Option<String>, features: Vec<String>) {
        self.selected_preset_id = preset_id;
        self.enabled_features = features;
        self.last_modified = std::time::Instant::now();
    }
}

// Helper functions for session state
fn get_session_state(installation_id: &str) -> Option<SessionInstallationState> {
    SESSION_STATE.lock().ok()?.get(installation_id).cloned()
}

fn set_session_state(installation_id: &str, preset_id: Option<String>, features: Vec<String>) {
    if let Ok(mut state) = SESSION_STATE.lock() {
        state.insert(
            installation_id.to_string(),
            SessionInstallationState::new(preset_id, features)
        );
    }
}

// Make this public so it can be called from other modules
pub fn clear_session_state(installation_id: &str) {
    if let Ok(mut state) = SESSION_STATE.lock() {
        state.remove(installation_id);
    }
}

#[component]
pub fn FeaturesTab(
    universal_manifest: Option<UniversalManifest>,
    presets: Vec<Preset>,
    enabled_features: Signal<Vec<String>>,
    selected_preset: Signal<Option<String>>,
    filter_text: Signal<String>,
    installation_id: String,
) -> Element {
    // Clone for closures - create all the clones we need upfront
    let presets_for_closure = presets.clone();
    let presets_for_toggle = presets.clone();
    let presets_for_custom_check = presets.clone();
    let installation_id_for_apply = installation_id.clone();
    let installation_id_for_toggle = installation_id.clone();
    let installation_id_for_session = installation_id.clone();
    let installation_id_for_effect = installation_id.clone();
    let universal_manifest_for_apply = universal_manifest.clone();
    let universal_manifest_for_toggle = universal_manifest.clone();
    let _universal_manifest_for_count = universal_manifest.clone();
    let universal_manifest_for_render = universal_manifest.clone();
    
    // Track if we've initialized from session state (not mutable)
    let session_initialized = use_signal(|| false);
    
    // Initialize preset state based on installation OR session state
    use_effect({
        let installation_id = installation_id_for_effect.clone();
        let mut selected_preset = selected_preset;
        let mut enabled_features = enabled_features;
        let mut session_initialized = session_initialized;
        
        move || {
            // Skip if already initialized from session
            if *session_initialized.read() {
                return;
            }
            
            // Load installation
            if let Ok(installation) = crate::installation::load_installation(&installation_id) {
                // Check if we have session state for this installation
                if let Some(session_state) = get_session_state(&installation_id) {
                    // Clone the values before using them
                    let session_features = session_state.enabled_features.clone();
                    let session_preset = session_state.selected_preset_id.clone();
                    
                    // Use session state (user's ongoing changes)
                    debug!("Restoring session state for installation {}", installation_id);
                    enabled_features.set(session_features.clone());
                    selected_preset.set(session_preset.clone());
                    
                    debug!("Restored from session - preset: {:?}, features: {:?}", 
                           session_preset, 
                           session_features.len());
                } else {
                    // No session state - use what's actually installed
                    debug!("No session state, using installed state for {}", installation_id);
                    enabled_features.set(installation.get_display_features());
                    selected_preset.set(installation.get_display_preset_id());
                    
                    debug!("Initialized from installation - preset: {:?}, features: {:?}", 
                           installation.get_display_preset_id(), 
                           installation.get_display_features());
                }
                
                session_initialized.set(true);
            }
        }
    });
    
    // Save to session state whenever features or preset changes
    use_effect({
        let installation_id = installation_id_for_session.clone();
        let selected_preset = selected_preset;
        let enabled_features = enabled_features;
        let session_initialized = session_initialized;
        
        move || {
            // Only save to session if we've initialized
            if *session_initialized.read() {
                let current_preset = selected_preset.read().clone();
                let current_features = enabled_features.read().clone();
                
                debug!("Saving session state for {} - preset: {:?}, features: {}", 
                       installation_id, current_preset, current_features.len());
                
                set_session_state(&installation_id, current_preset, current_features);
            }
        }
    });
    
    // Handle changing a preset
    let mut apply_preset = move |preset_id: String| {
        debug!("Applying preset: {}", preset_id);
        
        if preset_id == "custom" {
            // Custom preset: build default features list
            let mut default_features = vec!["default".to_string()];
            
            // Add any default-enabled features from the universal manifest
            if let Some(manifest) = &universal_manifest_for_apply {
                for component in &manifest.mods {
                    if component.default_enabled && component.id != "default" && !default_features.contains(&component.id) {
                        default_features.push(component.id.clone());
                    }
                }
                for component in &manifest.shaderpacks {
                    if component.default_enabled && component.id != "default" && !default_features.contains(&component.id) {
                        default_features.push(component.id.clone());
                    }
                }
                for component in &manifest.resourcepacks {
                    if component.default_enabled && component.id != "default" && !default_features.contains(&component.id) {
                        default_features.push(component.id.clone());
                    }
                }
                for include in &manifest.include {
                    if include.default_enabled && !include.id.is_empty() && include.id != "default" 
                       && !default_features.contains(&include.id) {
                        default_features.push(include.id.clone());
                    }
                }
                for remote in &manifest.remote_include {
                    if remote.default_enabled && remote.id != "default" 
                       && !default_features.contains(&remote.id) {
                        default_features.push(remote.id.clone());
                    }
                }
            }
            
            enabled_features.set(default_features.clone());
            selected_preset.set(None);
            
            // Save to session state
            set_session_state(&installation_id_for_apply, None, default_features.clone());
            
            // Save the selection immediately to installation (but don't install)
            if let Ok(mut installation) = crate::installation::load_installation(&installation_id_for_apply) {
                installation.save_pre_install_selections(None, default_features);
                installation.switch_to_custom_with_tracking();
                installation.modified = true;
                let _ = installation.save();
            }
        } else if let Some(preset) = find_preset_by_id(&presets_for_closure, &preset_id) {
            debug!("Found preset {} with features: {:?}", preset.name, preset.enabled_features);
            
            // Apply preset features
            enabled_features.set(preset.enabled_features.clone());
            selected_preset.set(Some(preset_id.clone()));
            
            // Save to session state
            set_session_state(&installation_id_for_apply, Some(preset_id.clone()), preset.enabled_features.clone());
            
            debug!("After applying preset, enabled features: {:?}", enabled_features.read());
            
            // Update installation
            if let Ok(mut installation) = crate::installation::load_installation(&installation_id_for_apply) {
                installation.save_pre_install_selections(Some(preset_id.clone()), preset.enabled_features.clone());
                installation.apply_preset_with_tracking(&preset);
                installation.modified = true;
                let _ = installation.save();
                
                debug!("Saved installation with features: {:?}", installation.enabled_features);
            }
        }
    };
    
    // Handle toggling a feature with dependency checking
    let toggle_feature = move |feature_id: String| {
        // Clone values at the start of the closure
        let manifest_for_deps = universal_manifest_for_toggle.clone();
        let installation_id_local = installation_id_for_toggle.clone();
        let presets_local = presets_for_toggle.clone();
        
        enabled_features.with_mut(|features| {
            let is_enabling = !features.contains(&feature_id);
            
            if is_enabling {
                if !features.contains(&feature_id) {
                    features.push(feature_id.clone());
                }
                
                // Check for dependencies and enable them too
                if let Some(manifest) = &manifest_for_deps {
                    let all_components: Vec<&ModComponent> = manifest.mods.iter()
                        .chain(manifest.shaderpacks.iter())
                        .chain(manifest.resourcepacks.iter())
                        .collect();
                    
                    if let Some(component) = all_components.iter().find(|c| c.id == feature_id) {
                        if let Some(deps) = &component.dependencies {
                            for dep_id in deps {
                                if !features.contains(dep_id) {
                                    debug!("Auto-enabling dependency: {} for {}", dep_id, feature_id);
                                    features.push(dep_id.clone());
                                }
                            }
                        }
                    }
                }
            } else {
                features.retain(|id| id != &feature_id);
                
                // Check if any enabled features depend on this one
                if let Some(manifest) = &manifest_for_deps {
                    let all_components: Vec<&ModComponent> = manifest.mods.iter()
                        .chain(manifest.shaderpacks.iter())
                        .chain(manifest.resourcepacks.iter())
                        .collect();
                    
                    let dependent_features: Vec<String> = all_components.iter()
                        .filter(|c| {
                            features.contains(&c.id) && 
                            c.dependencies.as_ref().is_some_and(|deps| deps.contains(&feature_id))
                        })
                        .map(|c| c.id.clone())
                        .collect();
                    
                    for dep_feat in dependent_features {
                        debug!("Auto-disabling dependent feature: {} (depends on {})", dep_feat, feature_id);
                        features.retain(|id| id != &dep_feat);
                    }
                }
            }
        });

        // Save to session state
        let current_preset = selected_preset.read().clone();
        let current_features = enabled_features.read().clone();
        set_session_state(&installation_id_local, current_preset, current_features);

        // Update installation with modification tracking
        if let Ok(mut installation) = crate::installation::load_installation(&installation_id_local) {
            let is_enabled = enabled_features.read().contains(&feature_id);
            installation.toggle_feature_with_tracking(&feature_id, is_enabled, &presets_local);
            installation.enabled_features = enabled_features.read().clone();
            installation.pending_features = enabled_features.read().clone();
            installation.modified = true;
            let _ = installation.save();
        }
    };
    
    // Button hover states
    let mut custom_button_hover = use_signal(|| false);
    let mut trending_button_hover = use_signal(Vec::<String>::new);
    let mut regular_button_hover = use_signal(Vec::<String>::new);
    
    // Features section expanded state
    let mut features_expanded = use_signal(|| false);
    
    // Find custom preset for the "Custom Configuration" card
    let custom_preset = presets_for_custom_check.iter().find(|p| p.id == "custom");
    
    rsx! {
        div { class: "features-tab",
            // PRESETS section header
            div { class: "section-divider with-title", 
                span { class: "divider-title", "PRESETS" }
            }
            
            p { class: "section-description", 
                "Choose a preset configuration or customize individual features below."
            }
            
            // Presets grid
            div { class: "presets-grid",
                // Custom preset (no preset selected)
                div { 
                    class: if selected_preset.read().is_none() {
                        "preset-card selected"
                    } else {
                        "preset-card"
                    },
                    style: {
                        if let Some(custom_preset) = custom_preset {
                            if let Some(bg) = &custom_preset.background {
                                format!("background-image: url('{}'); background-size: cover; background-position: center;", bg)
                            } else {
                                String::new()
                            }
                        } else {
                            String::new()
                        }
                    },
                    onclick: move |_| {
                        apply_preset("custom".to_string());
                    },
                                    
                    div { class: "preset-card-overlay" }
                    
                    div { class: "preset-card-content",
                        h4 { "CUSTOM OVERHAUL" }
                        p { "Start with default features and customize everything yourself." }
                    }
                    
                    // Select/Selected button
                    button {
                        class: "select-preset-button",
                        style: {
                            let is_selected = selected_preset.read().is_none();
                            let is_hovered = *custom_button_hover.read();
                            
                            if is_selected {
                                "background-color: white !important; color: #0a3d16 !important; border: none !important;"
                            } else if is_hovered {
                                "background-color: rgba(10, 80, 30, 0.9) !important; color: white !important; transform: translateX(-50%) translateY(-3px) !important; box-shadow: 0 5px 15px rgba(0, 0, 0, 0.4) !important;"
                            } else {
                                "background-color: rgba(7, 60, 23, 0.7) !important; color: white !important;"
                            }
                        },
                        onmouseenter: move |_| custom_button_hover.set(true),
                        onmouseleave: move |_| custom_button_hover.set(false),
                        
                        if selected_preset.read().is_none() {
                            "SELECTED"
                        } else {
                            "SELECT"
                        }
                    }
                }
                
                // Available presets - skip the "custom" preset since we handle it separately
                for preset in presets.iter().filter(|p| p.id != "custom") {
                    {
                        let preset_id = preset.id.clone();
                        let is_selected = selected_preset.read().as_ref() == Some(&preset_id);
                        let mut apply_preset_clone = apply_preset.clone();
                        let has_trending = preset.trending.unwrap_or(false);
                        
                        // Check if preset is updated
                        let is_updated = {
                            if let Ok(current_installation) = crate::installation::load_installation(&installation_id) {
                                if let Some(base_preset_id) = &current_installation.base_preset_id {
                                    if base_preset_id == &preset.id {
                                        if let (Some(preset_ver), Some(inst_ver)) = 
                                            (&preset.preset_version, &current_installation.base_preset_version) {
                                            preset_ver != inst_ver
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        };
                        
                        let button_id = preset_id.clone();
                        let is_button_hovered = if has_trending {
                            trending_button_hover.read().contains(&button_id)
                        } else {
                            regular_button_hover.read().contains(&button_id)
                        };
                        
                        rsx! {
                            div {
                                class: if is_selected {
                                    "preset-card selected"
                                } else {
                                    "preset-card"
                                },
                                style: if let Some(bg) = &preset.background {
                                    format!("background-image: url('{}'); background-size: cover; background-position: center;", bg)
                                } else {
                                    String::new()
                                },
                                onclick: move |_| {
                                    apply_preset_clone(preset_id.clone());
                                },
                                
                                span { class: "preset-features-count",
                                    "{preset.enabled_features.len()} features"
                                }
                                
                                if has_trending {
                                    span { class: "trending-badge", "Popular" }
                                }

                                if is_updated {
                                    span { class: "update-badge", "Updated" }
                                }
                                
                                div { class: "preset-card-overlay" }
                                
                                div { class: "preset-card-content",
                                    h4 { "{preset.name}" }
                                    p { "{preset.description}" }
                                }
                                
                                button {
                                    class: "select-preset-button",
                                    style: {
                                        if is_selected {
                                            if has_trending {
                                                "background-color: white !important; color: #b58c14 !important; border: none !important; box-shadow: 0 0 15px rgba(255, 179, 0, 0.3) !important;"
                                            } else {
                                                "background-color: white !important; color: #0a3d16 !important; border: none !important;"
                                            }
                                        } else if is_button_hovered {
                                            if has_trending {
                                                "background: linear-gradient(135deg, #e6b017, #cc9500) !important; color: black !important; transform: translateX(-50%) translateY(-3px) !important; box-shadow: 0 5px 15px rgba(0, 0, 0, 0.4) !important;"
                                            } else {
                                                "background-color: rgba(10, 80, 30, 0.9) !important; color: white !important; transform: translateX(-50%) translateY(-3px) !important; box-shadow: 0 5px 15px rgba(0, 0, 0, 0.4) !important;"
                                            }
                                        } else {
                                            if has_trending {
                                                "background: linear-gradient(135deg, #d4a017, #b78500) !important; color: black !important;"
                                            } else {
                                                "background-color: rgba(7, 60, 23, 0.7) !important; color: white !important;"
                                            }
                                        }
                                    },
                                    onmouseenter: {
                                        let button_id_enter = button_id.clone();
                                        let has_trending_enter = has_trending;
                                        move |_| {
                                            if has_trending_enter {
                                                trending_button_hover.with_mut(|ids| ids.push(button_id_enter.clone()));
                                            } else {
                                                regular_button_hover.with_mut(|ids| ids.push(button_id_enter.clone()));
                                            }
                                        }
                                    },
                                    onmouseleave: {
                                        let button_id_leave = button_id.clone();
                                        let has_trending_leave = has_trending;
                                        move |_| {
                                            if has_trending_leave {
                                                trending_button_hover.with_mut(|ids| ids.retain(|id| id != &button_id_leave));
                                            } else {
                                                regular_button_hover.with_mut(|ids| ids.retain(|id| id != &button_id_leave));
                                            }
                                        }
                                    },
                                    
                                    if is_selected {
                                        "SELECTED"
                                    } else {
                                        "SELECT"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // FEATURES section
            div { class: "optional-features-wrapper",
                div { class: "section-divider with-title", 
                    span { class: "divider-title", "FEATURES" }
                }
                
                p { class: "section-description", 
                    "Customize individual features to create your perfect experience."
                }
                                
                // Centered expand/collapse button
                button { 
                    class: "expand-collapse-button",
                    onclick: move |_| {
                        let current_expanded = *features_expanded.read();
                        features_expanded.set(!current_expanded);
                    },
                    
                    if *features_expanded.read() {
                        span { class: "button-icon collapse-icon", "▲" }
                        "Collapse Features"
                    } else {
                        span { class: "button-icon expand-icon", "▼" }
                        "Expand Features"
                    }
                }
                
                // Collapsible content
                div { 
                    class: if *features_expanded.read() {
                        "optional-features-content expanded"
                    } else {
                        "optional-features-content"
                    },
                    
                    // Search filter
                    div { class: "feature-filter-container",
                        span { class: "feature-filter-icon", "🔍" }
                        input {
                            class: "feature-filter",
                            placeholder: "Search for features...",
                            value: "{filter_text}",
                            oninput: move |evt| filter_text.set(evt.value().clone()),
                        }
                        
                        if !filter_text.read().is_empty() {
                            button {
                                class: "feature-filter-clear",
                                onclick: move |_| filter_text.set(String::new()),
                                "×"
                            }
                        }
                    }
                    
                    // Features content
                    {
                        if let Some(manifest) = &universal_manifest_for_render {
                            render_all_features_sections(
                                manifest.clone(),
                                enabled_features,
                                filter_text,
                                toggle_feature
                            )
                        } else {
                            rsx! {
                                div { class: "loading-container",
                                    div { class: "loading-spinner" }
                                    div { class: "loading-text", "Loading features..." }
                                }
                            }
                        }
                    }
                }
            }
        } 
    }
}

fn render_all_features_sections(
    manifest: UniversalManifest,
    enabled_features: Signal<Vec<String>>,
    filter_text: Signal<String>,
    toggle_feature: impl FnMut(String) + Clone + 'static,
) -> Element {
    let filter = filter_text.read().to_lowercase();
    
    // Collect all components INCLUDING includes and remote includes
    let mut all_components = Vec::new();
    all_components.extend(manifest.mods.iter().cloned());
    all_components.extend(manifest.shaderpacks.iter().cloned());
    all_components.extend(manifest.resourcepacks.iter().cloned());
    
    // Convert ALL includes to ModComponent format - FIXED VERSION
    for include in &manifest.include {
        // Process ALL includes regardless of optional status, but skip empty IDs
        if !include.id.is_empty() {
            debug!("Processing include: {} (optional: {}, default_enabled: {})", 
                   include.id, include.optional, include.default_enabled);
            
            all_components.push(ModComponent {
                id: include.id.clone(),
                name: include.name.clone().unwrap_or_else(|| {
                    // Better name extraction from location
                    include.location.split('/').next_back()
                        .unwrap_or(&include.location)
                        .trim_end_matches(".zip")
                        .trim_end_matches(".json")
                        .trim_end_matches(".txt")
                        .replace(['_', '-'], " ")
                }),
                description: include.description.clone()
                    .or_else(|| Some(format!("Configuration: {}", include.location))),
                source: "include".to_string(),
                location: include.location.clone(),
                version: "1.0".to_string(),
                path: None,
                optional: include.optional,
                default_enabled: include.default_enabled,
                authors: include.authors.clone().unwrap_or_default(),
                // CRITICAL FIX: Use the category from the include if it has one
                category: include.category.clone(),
                dependencies: include.dependencies.clone(),
                incompatibilities: None,
                ignore_update: include.ignore_update,
            });
        }
    }
    
    // Convert ALL remote includes to ModComponent format - FIXED VERSION
    for remote in &manifest.remote_include {
        debug!("Processing remote include: {} (optional: {}, default_enabled: {})", 
               remote.id, remote.optional, remote.default_enabled);
        
        all_components.push(ModComponent {
            id: remote.id.clone(),
            name: remote.name.clone().unwrap_or_else(|| {
                remote.id.replace(['_', '-'], " ")
            }),
            description: remote.description.clone().or_else(|| {
                Some(format!("Remote content from: {}", 
                    remote.location.split('/').next_back().unwrap_or("remote source")))
            }),
            source: "remote_include".to_string(),
            location: remote.location.clone(),
            version: remote.version.clone(),
            path: remote.path.as_ref().map(std::path::PathBuf::from),
            optional: remote.optional,
            default_enabled: remote.default_enabled,
            authors: remote.authors.clone(),
            // CRITICAL FIX: Use the actual category from the manifest
            category: remote.category.clone(),
            dependencies: remote.dependencies.clone(),
            incompatibilities: None,
            ignore_update: remote.ignore_update,
        });
    }
    
    debug!("Total components after includes: {}", all_components.len());
    debug!("Component IDs: {:?}", all_components.iter().map(|c| &c.id).collect::<Vec<_>>());
    
    // Filter components
    let filtered_components = if filter.is_empty() {
        all_components
    } else {
        all_components.into_iter()
            .filter(|comp| {
                let name_match = comp.name.to_lowercase().contains(&filter);
                let desc_match = comp.description.as_ref()
                    .is_some_and(|desc| desc.to_lowercase().contains(&filter));
                let category_match = comp.category.as_ref()
                    .is_some_and(|cat| cat.to_lowercase().contains(&filter));
                let id_match = comp.id.to_lowercase().contains(&filter);
                name_match || desc_match || category_match || id_match
            })
            .collect()
    };
    
    // Separate into included (default-enabled AND non-optional) and optional
    let (included_components, optional_components): (Vec<_>, Vec<_>) = filtered_components
        .into_iter()
        .partition(|comp| {
            // Component is included if:
            // 1. It's default_enabled AND not optional
            let is_included = comp.default_enabled && !comp.optional;
            
            debug!("Component {} - default_enabled: {}, optional: {}, included: {}", 
                   comp.id, comp.default_enabled, comp.optional, is_included);
            
            is_included
        });
    
    debug!("Included components: {}", included_components.len());
    debug!("Optional components: {}", optional_components.len());
    debug!("Optional component IDs: {:?}", optional_components.iter().map(|c| &c.id).collect::<Vec<_>>());
    
    // Group optional components by category
    let mut categories: std::collections::BTreeMap<String, Vec<ModComponent>> = std::collections::BTreeMap::new();
    for component in optional_components {
        let category = component.category.clone().unwrap_or_else(|| {
            // Only assign default categories if no category is specified
            match component.source.as_str() {
                "modrinth" => "Mods".to_string(),
                "ddl" | "mediafire" => {
                    // Try to infer from component type or location
                    if component.location.contains("shader") {
                        "Shaders".to_string()
                    } else if component.location.contains("resource") || component.location.contains("texture") {
                        "Resource Packs".to_string()
                    } else {
                        "Mods".to_string()
                    }
                },
                // For includes, try to infer from location
                "include" => {
                    if component.location.contains("config") {
                        "Configuration".to_string()
                    } else if component.location.contains("options") {
                        "Settings".to_string()
                    } else if component.location.contains("shader") {
                        "Shaders".to_string()
                    } else {
                        "Configuration".to_string()
                    }
                },
                "remote_include" => "Remote Content".to_string(),
                _ => "Other".to_string(),
            }
        });
        
        debug!("Adding component {} to category: {}", component.id, category);
        categories.entry(category).or_default().push(component);
    }
    
    debug!("Categories: {:?}", categories.keys().collect::<Vec<_>>());
    
    // Rest of the function remains the same...
    let mut included_expanded = use_signal(|| false);
    let mut expanded_categories = use_signal(Vec::<String>::new);
    
    // Check for no results
    let no_results = categories.is_empty() && included_components.is_empty() && !filter.is_empty();
    
    if no_results {
        return rsx! {
            div { class: "no-search-results",
                "No features found matching '{filter}'. Try a different search term."
            }
        };
    }
    
    rsx! {
        div { class: "feature-categories",
            // Included Features Section
            if !included_components.is_empty() {
                div { class: "feature-category",
                    div { 
                        class: "category-header",
                        onclick: move |_| {
                            included_expanded.with_mut(|expanded| *expanded = !*expanded);
                        },
                        
                        div { class: "category-title-section",
                            h3 { class: "category-name", "INCLUDED COMPONENTS" }
                            span { class: "category-count included-count", 
                                "{included_components.len()} included" 
                            }
                        }
                        
                        div { 
                            class: if *included_expanded.read() {
                                "category-toggle-indicator expanded"
                            } else {
                                "category-toggle-indicator"
                            },
                            "▼"
                        }
                    }
                    
                    div { 
                        class: if *included_expanded.read() {
                            "category-content expanded"
                        } else {
                            "category-content"
                        },
                        
                        div { class: "feature-cards-grid",
                            for component in included_components {
                                {
                                    rsx! {
                                        div { 
                                            class: "feature-card feature-included",
                                            
                                            div { class: "feature-card-header",
                                                h3 { class: "feature-card-title", "{component.name}" }
                                                
                                                span {
                                                    class: "feature-toggle-button included-component",
                                                    "Included"
                                                }
                                            }
                                            
                                            if let Some(description) = &component.description {
                                                div { class: "feature-card-description", "{description}" }
                                            }
                                            
                                            if !component.authors.is_empty() {
                                                div { class: "feature-authors",
                                                    "By: ",
                                                    for (i, author) in component.authors.iter().enumerate() {
                                                        {
                                                            let is_last = i == component.authors.len() - 1;
                                                            rsx! {
                                                                a {
                                                                    class: "author-link",
                                                                    href: "{author.link}",
                                                                    target: "_blank",
                                                                    "{author.name}"
                                                                }
                                                                if !is_last {
                                                                    ", "
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Optional Features by Category
            for (category_name, components) in categories {
                {
                    let category_key = category_name.clone();
                    let is_expanded = expanded_categories.read().contains(&category_key) 
                                     || !filter.is_empty();
                    
                    let enabled_count = enabled_features.read().iter()
                        .filter(|id| components.iter().any(|comp| &comp.id == *id))
                        .count();
                    
                    let are_all_enabled = !components.is_empty() && enabled_count == components.len();
                    
                    rsx! {
                        div { class: "feature-category",
                            div { 
                                class: "category-header",
                                onclick: {
                                    let category_key = category_key.clone();
                                    move |_| {
                                        expanded_categories.with_mut(|cats| {
                                            if cats.contains(&category_key) {
                                                cats.retain(|c| c != &category_key);
                                            } else {
                                                cats.push(category_key.clone());
                                            }
                                        });
                                    }
                                },
                                
                                div { class: "category-title-section",
                                    h3 { class: "category-name", "{category_name}" }
                                    span { class: "category-count", "{enabled_count}/{components.len()}" }
                                }
                                
                                // Toggle all button - has separate click handler
                                {
                                    let components_clone = components.clone();
                                    let mut enabled_features = enabled_features;
                                    
                                    rsx! {
                                        button {
                                            class: if are_all_enabled {
                                                "category-toggle-all toggle-disable"
                                            } else {
                                                "category-toggle-all toggle-enable"
                                            },
                                            onclick: move |evt| {
                                                // Stop propagation to prevent header's click handler
                                                evt.stop_propagation();
                                                
                                                // Toggle all in category (excluding default/included)
                                                enabled_features.with_mut(|features| {
                                                    if are_all_enabled {
                                                        // Disable all optional components
                                                        for comp in &components_clone {
                                                            if comp.id != "default" && comp.optional {
                                                                features.retain(|id| id != &comp.id);
                                                            }
                                                        }
                                                    } else {
                                                        // Enable all optional components
                                                        for comp in &components_clone {
                                                            if comp.id != "default" && comp.optional && !features.contains(&comp.id) {
                                                                features.push(comp.id.clone());
                                                            }
                                                        }
                                                    }
                                                });
                                            },
                                            
                                            if are_all_enabled {
                                                "Disable All"
                                            } else if enabled_count > 0 {
                                                "Enable All"
                                            } else {
                                                "Enable All"
                                            }
                                        }
                                    }
                                }
                                
                                // Expand/collapse indicator - larger, more visible
                                div { 
                                    class: if is_expanded {
                                        "category-toggle-indicator expanded"
                                    } else {
                                        "category-toggle-indicator"
                                    },
                                    "▼"
                                }
                            }
                            
                            // Category content (expandable)
                            div { 
                                class: if is_expanded {
                                    "category-content expanded"
                                } else {
                                    "category-content"
                                },
                                
                                // Feature cards grid
                                div { class: "feature-cards-grid",
                                    for component in components {
                                        {
                                            let component_id = component.id.clone();
                                            let is_enabled = enabled_features.read().contains(&component_id);
                                            let mut toggle_func = toggle_feature.clone();
                                            
                                            rsx! {
                                                div { 
                                                    class: if is_enabled {
                                                        "feature-card feature-enabled"
                                                    } else {
                                                        "feature-card feature-disabled"
                                                    },
                                                    
                                                    div { class: "feature-card-header",
                                                        h3 { class: "feature-card-title", "{component.name}" }
                                                        
                                                        // Special handling for default/included components
                                                        if component.id == "default" || !component.optional {
                                                            span {
                                                                class: "feature-toggle-button enabled default-component",
                                                                style: "cursor: default; opacity: 0.8;",
                                                                "Included"
                                                            }
                                                        } else {
                                                            label {
                                                                class: if is_enabled {
                                                                    "feature-toggle-button enabled"
                                                                } else {
                                                                    "feature-toggle-button disabled"
                                                                },
                                                                onclick: move |_| {
                                                                    toggle_func(component_id.clone());
                                                                },
                                                                
                                                                if is_enabled {
                                                                    "Enabled"
                                                                } else {
                                                                    "Disabled"
                                                                }
                                                            }
                                                        }
                                                    }
                                                    
                                                    // Description display
                                                    if let Some(description) = &component.description {
                                                        div { class: "feature-card-description", "{description}" }
                                                    }
                                                    
                                                    // Dependencies display
                                                    if let Some(deps) = &component.dependencies {
                                                        if !deps.is_empty() {
                                                            div { class: "feature-dependencies",
                                                                "Requires: ", 
                                                                span { class: "dependency-list", 
                                                                    {deps.join(", ")}
                                                                }
                                                            }
                                                        }
                                                    }
                                                    
                                                    // Authors display
                                                    if !component.authors.is_empty() {
                                                        div { class: "feature-authors",
                                                            "By: ",
                                                            for (i, author) in component.authors.iter().enumerate() {
                                                                {
                                                                    let is_last = i == component.authors.len() - 1;
                                                                    rsx! {
                                                                        a {
                                                                            class: "author-link",
                                                                            href: "{author.link}",
                                                                            target: "_blank",
                                                                            "{author.name}"
                                                                        }
                                                                        if !is_last {
                                                                            ", "
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
