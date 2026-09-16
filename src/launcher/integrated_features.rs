use dioxus::prelude::*;
use crate::preset::Preset;
use crate::universal::ModComponent;

// Component for the integrated presets and features section
#[derive(PartialEq, Props, Clone)]
pub struct IntegratedFeaturesProps {
    // Presets data
    pub presets: Vec<Preset>,
    pub selected_preset: Signal<Option<String>>,
    pub apply_preset: EventHandler<String>,
    
    // Features data
    pub mod_components: Vec<ModComponent>,
    pub enabled_features: Signal<Vec<String>>,
    pub toggle_feature: EventHandler<String>,
    
    // Optional filter
    pub filter_text: Option<Signal<String>>,
}

#[component]
pub fn IntegratedFeatures(mut props: IntegratedFeaturesProps) -> Element {
    // Create a signal to track expanded categories
    let mut expanded_categories = use_signal(Vec::<String>::new);
    
    // Group mods by category
    let mut categories: std::collections::BTreeMap<String, Vec<ModComponent>> = std::collections::BTreeMap::new();
    
    // Apply filter if present
    let filter = props.filter_text.as_ref().map(|signal| signal.read().to_lowercase());
    
    for component in &props.mod_components {
        // Skip if it doesn't match search filter
        if let Some(search_term) = &filter {
            if !search_term.is_empty() {
                let name_match = component.name.to_lowercase().contains(search_term);
                let description_match = component.description.as_ref()
                    .is_some_and(|desc| desc.to_lowercase().contains(search_term));
                    
                if !name_match && !description_match {
                    continue;
                }
            }
        }
        
        let category = component.category.clone().unwrap_or_else(|| "Uncategorized".to_string());
        categories.entry(category).or_default().push(component.clone());
    }
    
    // Check if no results match the filter
    let no_results = categories.is_empty() && filter.as_ref().is_some_and(|term| !term.is_empty());
    
    // Functions to toggle category expansion
    let mut toggle_category = move |category: String| {
        expanded_categories.with_mut(|cats| {
            if cats.contains(&category) {
                // Remove to collapse
                cats.retain(|c| c != &category);
            } else {
                // Add to expand
                cats.push(category);
            }
        });
    };
    
    // Function to toggle all features in a category
    let mut toggle_all_in_category = move |_category: &str, components: &[ModComponent], enable: bool| {
        // Get ids for all mods in this category
        let category_ids: Vec<String> = components.iter()
            .map(|comp| comp.id.clone())
            .collect();
            
        // Update enabled features
        props.enabled_features.with_mut(|features| {
            for id in &category_ids {
                if enable {
                    // Add if not present
                    if !features.contains(id) {
                        features.push(id.clone());
                    }
                } else {
                    // Remove if present
                    features.retain(|feat_id| feat_id != id);
                }
            }
        });
    };
    
    // Presets section first, followed by features categories
    rsx! {
        div { class: "features-container",
            // Presets section
            div { class: "presets-section",
                h3 { "Presets" }
                p { class: "presets-description", 
                    "Choose a preset configuration or customize individual features below."
                }
                
                div { class: "presets-grid",
                    // Custom preset (no preset)
                    div { 
                        class: if props.selected_preset.read().is_none() {
                            "preset-card selected"
                        } else {
                            "preset-card"
                        },
                        onclick: move |_| {
                            props.selected_preset.set(None);
                        },
                        
                        h4 { "Custom Configuration" }
                        p { "Start with your current selection and customize everything yourself." }
                    }
                    
                    // Available presets
                    for preset in &props.presets {
                        {
                            let preset_id = preset.id.clone();
                            let is_selected = props.selected_preset.read().as_ref() == Some(&preset_id);
                            
                            rsx! {
                                div {
                                    class: if is_selected {
                                        "preset-card selected"
                                    } else {
                                        "preset-card"
                                    },
                                    onclick: move |_| {
                                        props.apply_preset.call(preset_id.clone());
                                    },
                                    
                                    // Trending badge if applicable
                                    if preset.trending.unwrap_or(false) {
                                        span { class: "trending-badge", "Popular" }
                                    }
                                    
                                    h4 { "{preset.name}" }
                                    p { "{preset.description}" }
                                    
                                    // Feature count badge
                                    span { class: "preset-feature-count",
                                        "{preset.enabled_features.len()} features"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Show "no results" message if filter returns nothing
            if no_results {
                div { class: "no-search-results",
                    "No mods found matching '{filter.unwrap()}'. Try a different search term."
                }
            } else {
                // Feature categories
                div { class: "feature-categories",
                    for (category_name, components) in &categories {
                        {
                            let category_key = category_name.clone();
                            let is_expanded = expanded_categories.read().contains(&category_key);
                            
                            // Calculate how many components are enabled
                            let enabled_count = props.enabled_features.read().iter()
                                .filter(|id| components.iter().any(|comp| &comp.id == *id))
                                .count();
                            
                            let are_all_enabled = enabled_count == components.len();
                            
                            rsx! {
                                div { class: "feature-category",
                                    // Category header
                                    div { class: "category-header",
                                        div { class: "category-title-section",
                                            onclick: move |_| toggle_category(category_key.clone()),
                                            
                                            h3 { class: "category-name", "{category_name}" }
                                            span { class: "category-count", "{enabled_count}/{components.len()}" }
                                        }
                                        
                                        // Toggle all button
                                        {
                                            let components_clone = components.clone();
                                            let category_name_clone = category_name.clone();
                                            
                                            rsx! {
                                                button {
                                                    class: if are_all_enabled {
                                                        "category-toggle-all disabled"
                                                    } else {
                                                        "category-toggle-all"
                                                    },
                                                    onclick: move |_| {
                                                        toggle_all_in_category(&category_name_clone, &components_clone, !are_all_enabled);
                                                    },
                                                    
                                                    if are_all_enabled {
                                                        "Disable All"
                                                    } else {
                                                        "Enable All"
                                                    }
                                                }
                                            }
                                        }
                                        
                                        // Expand/collapse indicator
                                        {
                                            let category_key_clone = category_key.clone();
                                            
                                            rsx! {
                                                div { 
                                                    class: if is_expanded {
                                                        "category-toggle expanded"
                                                    } else {
                                                        "category-toggle"
                                                    },
                                                    onclick: move |_| toggle_category(category_key_clone.clone()),
                                                    "▼"
                                                }
                                            }
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
                                                    let is_enabled = props.enabled_features.read().contains(&component_id);
                                                    let toggle_feature = props.toggle_feature;
                                                    
                                                    rsx! {
                                                        div { 
                                                            class: if is_enabled {
                                                                "feature-card feature-enabled"
                                                            } else {
                                                                "feature-card feature-disabled"
                                                            },
                                                            
                                                            div { class: "feature-card-header",
                                                                h3 { class: "feature-card-title", "{component.name}" }
                                                                
                                                                label {
                                                                    class: if is_enabled {
                                                                        "feature-toggle-button enabled"
                                                                    } else {
                                                                        "feature-toggle-button disabled"
                                                                    },
                                                                    input {
                                                                        r#type: "checkbox",
                                                                        checked: is_enabled,
                                                                        onchange: move |_| toggle_feature.call(component_id.clone()),
                                                                        style: "display: none;"
                                                                    }
                                                                    
                                                                    if is_enabled {
                                                                        "Enabled"
                                                                    } else {
                                                                        "Disabled"
                                                                    }
                                                                }
                                                            }
                                                            
                                                            // Description display (truncated for compact layout)
                                                            if let Some(description) = &component.description {
                                                                div { class: "feature-card-description",
                                                                    // Truncate description if too long
                                                                    {
                                                                        if description.len() > 100 {
                                                                            format!("{}...", &description[..100])
                                                                        } else {
                                                                            description.clone()
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
