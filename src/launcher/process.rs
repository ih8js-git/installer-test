use std::process::Command;
use log::{debug, error, info};
use std::fmt;
use std::path::PathBuf;

use crate::launcher::launcher_finder::{get_launcher_path, find_mmc_like_launcher, MmcLikeLauncher, MmcLikeCommand};

#[derive(Debug)]
enum LauncherType {
    Vanilla,
    MultiMC,
    PrismLauncher,
    Custom(String),
}

// Implement Display trait for LauncherType
impl fmt::Display for LauncherType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LauncherType::Vanilla => write!(f, "Vanilla"),
            LauncherType::MultiMC => write!(f, "MultiMC"),
            LauncherType::PrismLauncher => write!(f, "Prism Launcher"),
            LauncherType::Custom(path) => write!(f, "Custom ({})", path),
        }
    }
}

// Main function to launch Minecraft with a specific profile
pub fn launch_modpack(profile_id: &str) -> Result<(), String> {
    // Determine which launcher we're using
    match get_current_launcher_type() {
        Ok(LauncherType::Vanilla) => launch_vanilla(profile_id),
        Ok(LauncherType::MultiMC) => launch_multimc(profile_id),
        Ok(LauncherType::PrismLauncher) => launch_prism(profile_id),
        Ok(LauncherType::Custom(path)) => launch_custom_multimc(profile_id, path),
        Err(e) => Err(e),
    }
}

// Determine which launcher we're using
fn get_current_launcher_type() -> Result<LauncherType, String> {
    // Read config to determine current launcher
    let config = match std::fs::read_to_string(crate::get_app_data().join(".WC_OVHL/config.json")) {
        Ok(content) => content,
        Err(e) => return Err(format!("Failed to read config: {}", e)),
    };
    
    let config: serde_json::Value = match serde_json::from_str(&config) {
        Ok(parsed) => parsed,
        Err(e) => return Err(format!("Failed to parse config: {}", e)),
    };
    
    let launcher = match config["launcher"].as_str() {
        Some(val) => val,
        None => return Err("Launcher not specified in config".to_string()),
    };
    
    match launcher {
        "vanilla" => Ok(LauncherType::Vanilla),
        "multimc-MultiMC" => Ok(LauncherType::MultiMC),
        "multimc-PrismLauncher" => Ok(LauncherType::PrismLauncher),
        custom if custom.starts_with("custom-") => {
            let path = custom.trim_start_matches("custom-").to_string();
            Ok(LauncherType::Custom(path))
        },
        _ => Err(format!("Unknown launcher type: {}", launcher)),
    }
}

// Launch vanilla Minecraft with the specified profile
fn launch_vanilla(profile_id: &str) -> Result<(), String> {
    debug!("Launching vanilla Minecraft for profile {}", profile_id);
    
    // Get Minecraft directory
    let minecraft_dir = crate::get_minecraft_folder();
    
    // Update the launcher profiles to make this the default
    let profiles_path = minecraft_dir.join("launcher_profiles.json");
    
    if profiles_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&profiles_path) {
            if let Ok(mut profiles_json) = serde_json::from_str::<serde_json::Value>(&content) {
                // Update lastUsed timestamp
                let now = chrono::Utc::now().to_rfc3339();
                
                if let Some(profiles) = profiles_json.get_mut("profiles") {
                    if let Some(profile) = profiles.get_mut(profile_id) {
                        if let Some(profile_obj) = profile.as_object_mut() {
                            profile_obj.insert("lastUsed".to_string(), serde_json::Value::String(now));
                            debug!("Updated lastUsed timestamp for profile {}", profile_id);
                        }
                    }
                }
                
                // Set as the selected profile
                if let Some(obj) = profiles_json.as_object_mut() {
                    if let Some(selected_profile) = obj.get_mut("selectedProfileId") {
                        *selected_profile = serde_json::Value::String(profile_id.to_string());
                        debug!("Set profile {} as selected", profile_id);
                    } else {
                        obj.insert(
                            "selectedProfileId".to_string(),
                            serde_json::Value::String(profile_id.to_string()),
                        );
                        debug!("Added selectedProfileId with profile {}", profile_id);
                    }
                }
                
                // Write back the updated profiles
                if let Ok(updated_json) = serde_json::to_string_pretty(&profiles_json) {
                    if let Err(e) = std::fs::write(&profiles_path, updated_json) {
                        debug!("Failed to write updated profiles: {}", e);
                    }
                }
            }
        }
    }
    
    // Find the launcher executable using our robust finder
    let launcher_path: PathBuf = match get_launcher_path() {
        Ok(path) => path,
        Err(e) => {
            error!("Failed to find Minecraft launcher: {}", e);
            return Err(format!(
                "{}

Please try one of the following:
1. Install Minecraft from minecraft.net
2. Make sure the Minecraft Launcher is installed
3. Manually launch Minecraft and select the profile '{}'

If Minecraft is installed in a non-standard location, the launcher may not be able to find it automatically.",
                e, profile_id
            ));
        }
    };
    
    info!("Launching Minecraft from: {}", launcher_path.display());
    
    match Command::new(&launcher_path).spawn() {
        Ok(_) => {
            info!("Minecraft launcher started successfully");
            Ok(())
        },
        Err(e) => {
            error!("Failed to start Minecraft launcher: {}", e);
            Err(format!(
                "Failed to start Minecraft launcher: {}

The launcher was found at: {}

Please try:
1. Running the launcher manually from this location
2. Checking if you have permission to execute it
3. Manually selecting the profile '{}' in Minecraft

If problems persist, you may need to reinstall Minecraft.",
                e, launcher_path.display(), profile_id
            ))
        }
    }
}

// Launch MultiMC with the specified instance
fn launch_multimc(profile_id: &str) -> Result<(), String> {
    let cmd = find_mmc_like_launcher(MmcLikeLauncher::MultiMC).ok_or_else(|| {
        format!(
            "Could not find the MultiMC executable on this system.\n\n\
            Please launch MultiMC manually and select the instance '{}'.",
            profile_id
        )
    })?;

    debug!("Launching MultiMC with instance {} via {:?}", profile_id, cmd);

    let spawn_result = match &cmd {
        // MultiMC's own launch flag is "-l"
        MmcLikeCommand::Direct(path) => Command::new(path).arg("-l").arg(profile_id).spawn(),
        MmcLikeCommand::Flatpak(app_id) => Command::new("flatpak")
            .arg("run")
            .arg(app_id)
            .arg("-l")
            .arg(profile_id)
            .spawn(),
    };

    match spawn_result {
        Ok(_) => {
            debug!("MultiMC launched successfully with instance: {}", profile_id);
            Ok(())
        }
        Err(e) => {
            error!("Failed to start MultiMC: {}", e);
            Err(format!("Failed to start MultiMC: {}", e))
        }
    }
}

// Launch Prism Launcher with the specified instance
fn launch_prism(profile_id: &str) -> Result<(), String> {
    let cmd = find_mmc_like_launcher(MmcLikeLauncher::Prism).ok_or_else(|| {
        format!(
            "Could not find the Prism Launcher executable on this system.\n\n\
            Please launch Prism Launcher manually and select the instance '{}'.",
            profile_id
        )
    })?;

    debug!("Launching Prism Launcher with instance {} via {:?}", profile_id, cmd);

    let spawn_result = match &cmd {
        MmcLikeCommand::Direct(path) => Command::new(path).arg("-l").arg(profile_id).spawn(),
        MmcLikeCommand::Flatpak(app_id) => Command::new("flatpak")
            .arg("run")
            .arg(app_id)
            .arg("-l")
            .arg(profile_id)
            .spawn(),
    };

    match spawn_result {
        Ok(_) => {
            debug!("Prism Launcher launched successfully with instance: {}", profile_id);
            Ok(())
        }
        Err(e) => {
            error!("Failed to start Prism Launcher: {}", e);
            Err(format!("Failed to start Prism Launcher: {}", e))
        }
    }
}

// Launch custom MultiMC with the specified instance
fn launch_custom_multimc(profile_id: &str, path: String) -> Result<(), String> {
    // Convert path string to PathBuf
    let custom_path = std::path::PathBuf::from(path);
    
    // Try to determine the executable name
    let executable = if cfg!(target_os = "windows") {
        // Look for common executables
        if custom_path.join("MultiMC.exe").exists() {
            custom_path.join("MultiMC.exe")
        } else if custom_path.join("prismlauncher.exe").exists() {
            custom_path.join("prismlauncher.exe")
        } else {
            // Default to MultiMC.exe and hope for the best
            custom_path.join("MultiMC.exe")
        }
    } else if cfg!(target_os = "macos") {
        // Look for common executables
        if custom_path.join("MultiMC.app/Contents/MacOS/MultiMC").exists() {
            custom_path.join("MultiMC.app/Contents/MacOS/MultiMC")
        } else if custom_path.join("prismlauncher.app/Contents/MacOS/prismlauncher").exists() {
            custom_path.join("prismlauncher.app/Contents/MacOS/prismlauncher")
        } else {
            // Default to MultiMC and hope for the best
            custom_path.join("MultiMC.app/Contents/MacOS/MultiMC")
        }
    } else {
        // Linux: look for common executables
        if custom_path.join("MultiMC").exists() {
            custom_path.join("MultiMC")
        } else if custom_path.join("prismlauncher").exists() {
            custom_path.join("prismlauncher")
        } else {
            // Default to MultiMC and hope for the best
            custom_path.join("MultiMC")
        }
    };
    
    debug!("Launching custom MultiMC-like launcher with instance {}", profile_id);
    
    // Launch with the instance
    match Command::new(executable)
        .arg("-l")
        .arg(profile_id)
        .spawn() {
            Ok(_) => {
                debug!("Custom launcher launched successfully with instance: {}", profile_id);
                Ok(())
            },
            Err(e) => {
                error!("Failed to start custom launcher: {}", e);
                Err(format!("Failed to start custom launcher: {}", e))
            }
        }
}
