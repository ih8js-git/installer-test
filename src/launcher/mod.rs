use log::debug;

pub mod config;
mod process;

// Correct imports
pub use config::{update_jvm_args, get_jvm_args};
pub use process::launch_modpack;

// Component modules - features_tab remains public
mod integrated_features;
pub mod features_tab;
mod performance_tab;
mod settings_tab;

mod launcher_finder;

// Updated exports
pub use features_tab::FeaturesTab;
pub use performance_tab::PerformanceTab;
pub use settings_tab::SettingsTab;

// Define public feature types needed by other modules
pub struct FeatureCard;
pub struct FeatureCategory;
pub struct FeatureFilter;

pub fn update_launcher_profile_jvm_args(installation_id: &str, java_args: &str) -> Result<(), String> {
    use std::fs;
    
    let minecraft_dir = crate::get_minecraft_folder();
    let profiles_path = minecraft_dir.join("launcher_profiles.json");
    
    if !profiles_path.exists() {
        return Err("launcher_profiles.json not found".to_string());
    }
    
    let content = fs::read_to_string(&profiles_path)
        .map_err(|e| format!("Failed to read launcher profiles: {}", e))?;
    
    let mut profiles: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse launcher profiles: {}", e))?;
    
    if let Some(profiles_obj) = profiles.get_mut("profiles").and_then(|p| p.as_object_mut()) {
        if let Some(profile) = profiles_obj.get_mut(installation_id) {
            if let Some(profile_obj) = profile.as_object_mut() {
                profile_obj.insert("javaArgs".to_string(), serde_json::Value::String(java_args.to_string()));
                debug!("Updated launcher profile {} with JVM args: {}", installation_id, java_args);
            }
        } else {
            return Err(format!("Profile {} not found in launcher", installation_id));
        }
    }
    
    let updated_json = serde_json::to_string_pretty(&profiles)
        .map_err(|e| format!("Failed to serialize profiles: {}", e))?;
    
    fs::write(&profiles_path, updated_json)
        .map_err(|e| format!("Failed to write launcher profiles: {}", e))?;
    
    Ok(())
}
