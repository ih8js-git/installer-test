use std::path::PathBuf;
use std::fs;
use serde_json::{Value, json};
use log::{debug, warn};

// Default optimized JVM args
pub const DEFAULT_JVM_ARGS: &str = "-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M -Xmx4G";

// Get the Minecraft directory
pub fn get_minecraft_dir() -> PathBuf {
    let home = dirs::home_dir().expect("Could not find home directory");
    
    #[cfg(target_os = "windows")]
    let mc_dir = home.join("AppData").join("Roaming").join(".minecraft");
    
    #[cfg(target_os = "macos")]
    let mc_dir = home.join("Library").join("Application Support").join("minecraft");
    
    #[cfg(target_os = "linux")]
    let mc_dir = home.join(".minecraft");
    
    mc_dir
}

// Get launcher_profiles.json path
fn get_profiles_path() -> PathBuf {
    get_minecraft_dir().join("launcher_profiles.json")
}

// Read the current launcher profiles
pub fn read_profiles() -> Result<Value, String> {
    let path = get_profiles_path();
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read launcher profiles: {}", e))?;
    
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse launcher profiles: {}", e))
}

// Update the JVM arguments for a specific profile
pub fn update_jvm_args(profile_id: &str, jvm_args: &str) -> Result<(), String> {
    let mut profiles = read_profiles()?;
    
    // Ensure the profiles object exists
    if !profiles["profiles"].is_object() {
        return Err("Invalid launcher_profiles.json format".into());
    }
    
    // Update or create the profile
    let profile_exists = profiles["profiles"].as_object()
        .unwrap()
        .contains_key(profile_id);
    
    if profile_exists {
        profiles["profiles"][profile_id]["javaArgs"] = json!(jvm_args);
    } else {
        // Profile doesn't exist - create it
        let new_profile = json!({
            "name": format!("Wynncraft Overhaul - {}", profile_id),
            "type": "custom",
            "created": chrono::Utc::now().to_rfc3339(),
            "lastUsed": chrono::Utc::now().to_rfc3339(),
            "icon": "Furnace",
            "javaArgs": jvm_args,
            "gameDir": format!("{}", get_minecraft_dir().join(format!("instances/{}", profile_id)).display())
        });
        
        if let Some(profiles_obj) = profiles["profiles"].as_object_mut() {
            profiles_obj.insert(profile_id.to_string(), new_profile);
        }
    }
    
    // Write the updated profiles back to the file
    let json_str = serde_json::to_string_pretty(&profiles)
        .map_err(|e| format!("Failed to serialize profiles: {}", e))?;
    
    fs::write(get_profiles_path(), json_str)
        .map_err(|e| format!("Failed to write launcher profiles: {}", e))?;
    
    debug!("Updated JVM args for profile {}: {}", profile_id, jvm_args);
    Ok(())
}

// Get current JVM args for a profile (returns default if not found)
pub fn get_jvm_args(profile_id: &str) -> Result<String, String> {
    // First check if we have custom arguments in our own config
    let app_data = crate::get_app_data();
    let custom_args_path = app_data.join(format!(".WC_OVHL/{}/jvm_args.txt", profile_id));
    
    if custom_args_path.exists() {
        match fs::read_to_string(&custom_args_path) {
            Ok(args) if !args.trim().is_empty() => {
                debug!("Using custom JVM args from {}: {}", custom_args_path.display(), args);
                return Ok(args);
            },
            _ => {
                debug!("Custom JVM args file exists but is empty or unreadable");
            }
        }
    }
    
    // Check for JVM args in the manifest
    let manifest_path = app_data.join(format!(".WC_OVHL/{}/manifest.json", profile_id));
    if manifest_path.exists() {
        if let Ok(content) = fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<Value>(&content) {
                // Check for manifest-specified JVM args
                if let Some(java_args) = manifest["java_args"].as_str() {
                    debug!("Using JVM args from manifest: {}", java_args);
                    return Ok(java_args.to_string());
                }
                
                // Check for max_mem and min_mem
                let mut args = String::new();
                
                // Base optimization flags
                args.push_str("-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M");
                
                if let Some(max_mem) = manifest["max_mem"].as_i64() {
                    args.push_str(&format!(" -Xmx{}M", max_mem));
                } else {
                    args.push_str(" -Xmx4G");
                }
                
                if let Some(min_mem) = manifest["min_mem"].as_i64() {
                    args.push_str(&format!(" -Xms{}M", min_mem));
                }
                
                debug!("Generated JVM args from manifest settings: {}", args);
                return Ok(args);
            }
        }
    }
    
    // Try checking in launcher_profiles.json
    let profiles = match read_profiles() {
        Ok(profiles) => profiles,
        Err(_) => return Ok(DEFAULT_JVM_ARGS.to_string()),
    };
    
    if let Some(profile) = profiles["profiles"].get(profile_id) {
        if let Some(args) = profile["javaArgs"].as_str() {
            debug!("Found JVM args in launcher profile: {}", args);
            return Ok(args.to_string());
        }
    }
    
    // Return default if not found
    debug!("No specific JVM args found, using default: {}", DEFAULT_JVM_ARGS);
    Ok(DEFAULT_JVM_ARGS.to_string())
}

pub fn update_memory_allocation(installation_id: &str, memory_mb: i32) -> Result<(), String> {
    debug!("Updating memory allocation for {} to {}MB", installation_id, memory_mb);
    
    // Load the installation
    let app_data = get_app_data_dir();
    let installation_dir = app_data.join(format!(".WC_OVHL/installations/{}", installation_id));
    let config_path = installation_dir.join("installation.json");
    
    if !config_path.exists() {
        return Err("Installation not found".to_string());
    }
    
    // Read and update the installation config
    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read installation config: {}", e))?;
    
    let mut installation: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse installation config: {}", e))?;
    
    // Update memory allocation field
    if let Some(obj) = installation.as_object_mut() {
        obj.insert("memory_allocation".to_string(), serde_json::Value::Number(serde_json::Number::from(memory_mb)));
        
        // Update Java args - remove any existing -Xmx and -Xms
        if let Some(java_args) = obj.get("java_args").and_then(|v| v.as_str()) {
            let mut parts: Vec<&str> = java_args.split_whitespace().collect();
            // Remove any existing memory arguments
            parts.retain(|part| !part.starts_with("-Xmx") && !part.starts_with("-Xms"));
            
            // Add the new memory parameter (only -Xmx, no -Xms)
            let memory_param = if memory_mb >= 1024 {
                format!("-Xmx{}G", memory_mb / 1024)
            } else {
                format!("-Xmx{}M", memory_mb)
            };
            
            parts.push(&memory_param);
            
            // Join and update
            let updated_args = parts.join(" ");
            obj.insert("java_args".to_string(), serde_json::Value::String(updated_args));
            
            debug!("Updated Java args: {}", obj.get("java_args").unwrap());
        }
    }
    
    // Write back
    let updated_json = serde_json::to_string_pretty(&installation)
        .map_err(|e| format!("Failed to serialize installation: {}", e))?;
    
    fs::write(&config_path, updated_json)
        .map_err(|e| format!("Failed to write installation config: {}", e))?;
    
    // Also update the launcher profile
    update_launcher_profile_memory(installation_id, memory_mb)?;
    
    Ok(())
}

// Get the JVM args for an installation
pub fn get_installation_jvm_args(installation_id: &str) -> Result<String, String> {
    let app_data = get_app_data_dir();
    let installation_dir = app_data.join(format!(".WC_OVHL/installations/{}", installation_id));
    
    // First try to read from the installation's config
    let config_path = installation_dir.join("installation.json");
    if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(json) => {
                        if let Some(java_args) = json["java_args"].as_str() {
                            return Ok(java_args.to_string());
                        }
                    },
                    Err(e) => warn!("Failed to parse installation config: {}", e)
                }
            },
            Err(e) => warn!("Failed to read installation config: {}", e)
        }
    }

    // Fallback to default if not found
    Ok("-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M -Xmx4G".to_string())
}

// Save JVM args for an installation
fn save_jvm_args(installation_id: &str, args: &str) -> Result<(), String> {
    let app_data = get_app_data_dir();
    let installation_dir = app_data.join(format!(".WC_OVHL/installations/{}", installation_id));
    
    // Make sure directory exists
    fs::create_dir_all(&installation_dir)
        .map_err(|e| format!("Failed to create installation directory: {}", e))?;
    
    // Try to update the installation's config file
    let config_path = installation_dir.join("installation.json");
    if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(mut json) => {
                        if let Some(obj) = json.as_object_mut() {
                            obj.insert("java_args".to_string(), serde_json::Value::String(args.to_string()));
                            
                            match serde_json::to_string_pretty(&json) {
                                Ok(updated_json) => {
                                    match fs::write(&config_path, updated_json) {
                                        Ok(_) => {
                                            debug!("Updated JVM args in installation config");
                                            return Ok(());
                                        },
                                        Err(e) => warn!("Failed to write updated config: {}", e)
                                    }
                                },
                                Err(e) => warn!("Failed to serialize updated config: {}", e)
                            }
                        }
                    },
                    Err(e) => warn!("Failed to parse installation config: {}", e)
                }
            },
            Err(e) => warn!("Failed to read installation config: {}", e)
        }
    }
    
    // If we couldn't update the config file, save to a separate file
    let args_path = installation_dir.join("jvm_args.txt");
    match fs::write(&args_path, args) {
        Ok(_) => {
            debug!("Saved JVM args to separate file");
            Ok(())
        },
        Err(e) => Err(format!("Failed to save JVM args: {}", e))
    }
}

// Helper to get app data directory
fn get_app_data_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
    } else if cfg!(target_os = "macos") {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        // Linux
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }
}

// Extract current memory value from JVM args
pub fn extract_memory_from_args(args: &str) -> Option<i32> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    
    for part in parts {
        if let Some(mem_str) = part.strip_prefix("-Xmx") {
            // Remove "-Xmx" prefix
            
            // Check for GB format
            if mem_str.ends_with('G') || mem_str.ends_with('g') {
                if let Ok(gb) = mem_str[..mem_str.len()-1].parse::<i32>() {
                    return Some(gb * 1024); // Convert GB to MB
                }
            }
            // Check for MB format
            else if mem_str.ends_with('M') || mem_str.ends_with('m') {
                if let Ok(mb) = mem_str[..mem_str.len()-1].parse::<i32>() {
                    return Some(mb);
                }
            }
            // Try parsing as just a number (assumed MB)
            else if let Ok(mb) = mem_str.parse::<i32>() {
                return Some(mb);
            }
        }
    }
    
    // Default to 4GB if not found
    Some(4 * 1024)
}

pub fn update_launcher_profile_memory(installation_id: &str, memory_mb: i32) -> Result<(), String> {
    use std::fs;
    use serde_json::Value;
    
    let minecraft_dir = crate::get_minecraft_folder();
    let profiles_path = minecraft_dir.join("launcher_profiles.json");
    
    if !profiles_path.exists() {
        return Err("launcher_profiles.json not found".to_string());
    }
    
    let content = fs::read_to_string(&profiles_path)
        .map_err(|e| format!("Failed to read launcher profiles: {}", e))?;
    
    let mut profiles: Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse launcher profiles: {}", e))?;
    
    if let Some(profiles_obj) = profiles.get_mut("profiles").and_then(|p| p.as_object_mut()) {
        if let Some(profile) = profiles_obj.get_mut(installation_id) {
            if let Some(profile_obj) = profile.as_object_mut() {
                // Get current javaArgs or use default
                let current_args = profile_obj.get("javaArgs")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M");
                
                // Parse and rebuild args
                let mut args_parts: Vec<String> = Vec::new();
                
                // Keep non-memory args
                for arg in current_args.split_whitespace() {
                    if !arg.starts_with("-Xmx") && !arg.starts_with("-Xms") {
                        args_parts.push(arg.to_string());
                    }
                }
                
                // Add new memory setting (ONLY -Xmx, NO -Xms)
                if memory_mb >= 1024 && memory_mb % 1024 == 0 {
                    args_parts.push(format!("-Xmx{}G", memory_mb / 1024));
                } else {
                    args_parts.push(format!("-Xmx{}M", memory_mb));
                }
                
                let final_args = args_parts.join(" ");
                profile_obj.insert("javaArgs".to_string(), Value::String(final_args.clone()));
                
                debug!("Updated launcher profile {} with JVM args: {}", installation_id, final_args);
            }
        }
    }
    
    let updated_json = serde_json::to_string_pretty(&profiles)
        .map_err(|e| format!("Failed to serialize profiles: {}", e))?;
    
    fs::write(&profiles_path, updated_json)
        .map_err(|e| format!("Failed to write launcher profiles: {}", e))?;
    
    Ok(())
}
