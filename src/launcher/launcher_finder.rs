use std::path::{Path, PathBuf};
use log::{debug, info, warn};

/// Find the Minecraft launcher executable on Windows
#[cfg(target_os = "windows")]
pub fn find_minecraft_launcher() -> Option<PathBuf> {
    // Common installation paths for Minecraft launcher
    // Note: Xbox Game Pass uses "Minecraft.exe" instead of "MinecraftLauncher.exe"
    let search_paths = vec![
        // Standard Program Files locations
        "C:\\Program Files (x86)\\Minecraft Launcher\\MinecraftLauncher.exe",
        "C:\\Program Files\\Minecraft Launcher\\MinecraftLauncher.exe",
        
        // Microsoft Store version
        "C:\\Program Files\\WindowsApps\\Microsoft.MinecraftLauncher_1.0.0.0_x64__8wekyb3d8bbwe\\Minecraft.exe",
        
        // Xbox Game Pass locations (uses Minecraft.exe, not MinecraftLauncher.exe)
        "C:\\XboxGames\\Minecraft Launcher\\Content\\Minecraft.exe",
        "C:\\XboxGames\\Minecraft Launcher\\Content\\MinecraftLauncher.exe",
        "C:\\Program Files\\XboxGames\\Minecraft Launcher\\Content\\Minecraft.exe",
        "C:\\Program Files\\XboxGames\\Minecraft Launcher\\Content\\MinecraftLauncher.exe",
        
        // Legacy locations
        "C:\\Program Files (x86)\\Minecraft\\MinecraftLauncher.exe",
        "C:\\Program Files\\Minecraft\\MinecraftLauncher.exe",
    ];
    
    // First, try the standard paths
    for path_str in &search_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            debug!("Found Minecraft launcher at: {}", path.display());
            return Some(path);
        }
    }
    
    // Try to find via registry (Windows only)
    if let Some(path) = find_launcher_via_registry() {
        return Some(path);
    }
    
    // Search in user's home directory (Downloads, Desktop, Documents)
    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        let user_dir = PathBuf::from(user_profile);
        
        let user_search_paths = vec![
            // Standard launcher locations
            user_dir.join("Downloads\\MinecraftLauncher.exe"),
            user_dir.join("Desktop\\MinecraftLauncher.exe"),
            user_dir.join("Documents\\MinecraftLauncher.exe"),
            user_dir.join("Downloads\\Minecraft Launcher\\MinecraftLauncher.exe"),
            user_dir.join("Desktop\\Minecraft Launcher\\MinecraftLauncher.exe"),
            
            // Xbox Game Pass alternate executable name
            user_dir.join("Downloads\\Minecraft.exe"),
            user_dir.join("Desktop\\Minecraft.exe"),
            user_dir.join("Documents\\Minecraft.exe"),
            user_dir.join("Downloads\\Minecraft Launcher\\Minecraft.exe"),
            user_dir.join("Desktop\\Minecraft Launcher\\Minecraft.exe"),
            
            // AppData locations
            user_dir.join("AppData\\Local\\Packages\\Microsoft.4297127D64EC6_8wekyb3d8bbwe\\LocalCache\\Local\\Minecraft\\MinecraftLauncher.exe"),
            user_dir.join("AppData\\Local\\Packages\\Microsoft.MinecraftUWP_8wekyb3d8bbwe\\LocalState\\MinecraftLauncher.exe"),
            user_dir.join("AppData\\Local\\Packages\\Microsoft.MinecraftUWP_8wekyb3d8bbwe\\LocalState\\Minecraft.exe"),
        ];
        
        for path in user_search_paths {
            if path.exists() {
                debug!("Found Minecraft launcher in user directory: {}", path.display());
                return Some(path);
            }
        }
    }
    
    // Last resort: search common drive letters
    for drive in &['C', 'D', 'E'] {
        if let Some(path) = search_drive_for_launcher(*drive) {
            return Some(path);
        }
    }
    
    warn!("Could not find Minecraft launcher in any common location");
    None
}

/// Search a specific drive for the Minecraft launcher (limited depth to avoid long searches)
#[cfg(target_os = "windows")]
fn search_drive_for_launcher(drive: char) -> Option<PathBuf> {
    use std::fs;
    
    let root = PathBuf::from(format!("{}:\\", drive));
    if !root.exists() {
        return None;
    }
    
    // Search in common directories to keep search fast
    let search_dirs = vec![
        root.join("Program Files (x86)"),
        root.join("Program Files"),
        root.join("XboxGames"),  // Xbox Game Pass installations
    ];
    
    for dir in search_dirs {
        if !dir.exists() {
            continue;
        }
        
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy();
                    if name_str.to_lowercase().contains("minecraft") {
                        // Check for both possible executable names
                        let possible_paths = vec![
                            path.join("MinecraftLauncher.exe"),
                            path.join("Minecraft.exe"),
                            path.join("Content\\MinecraftLauncher.exe"),
                            path.join("Content\\Minecraft.exe"),
                        ];
                        
                        for launcher_path in possible_paths {
                            if launcher_path.exists() {
                                debug!("Found Minecraft launcher at: {}", launcher_path.display());
                                return Some(launcher_path);
                            }
                        }
                    }
                }
            }
        }
    }
    
    None
}

/// Try to find the launcher via Windows registry
#[cfg(target_os = "windows")]
fn find_launcher_via_registry() -> Option<PathBuf> {
    use winreg::enums::*;
    use winreg::RegKey;
    
    // Try to read from registry where Minecraft might store its installation path
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    
    let registry_paths = vec![
        "SOFTWARE\\Mojang\\InstalledProducts\\Minecraft Launcher",
        "SOFTWARE\\WOW6432Node\\Mojang\\InstalledProducts\\Minecraft Launcher",
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Minecraft Launcher",
    ];
    
    for reg_path in registry_paths {
        if let Ok(key) = hklm.open_subkey(reg_path) {
            // Try to read InstallLocation or similar keys
            for value_name in &["InstallLocation", "InstallPath", "Path", ""] {
                if let Ok(install_path) = key.get_value::<String, _>(value_name) {
                    let launcher_path = PathBuf::from(install_path).join("MinecraftLauncher.exe");
                    if launcher_path.exists() {
                        debug!("Found Minecraft launcher via registry: {}", launcher_path.display());
                        return Some(launcher_path);
                    }
                }
            }
        }
    }
    
    None
}

/// macOS implementation
#[cfg(target_os = "macos")]
pub fn find_minecraft_launcher() -> Option<PathBuf> {
    let search_paths = vec![
        "/Applications/Minecraft.app/Contents/MacOS/launcher",
        "/Applications/Minecraft.app",
    ];
    
    for path_str in &search_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            debug!("Found Minecraft launcher at: {}", path.display());
            return Some(path);
        }
    }
    
    // Check user's Applications folder
    if let Some(home) = dirs::home_dir() {
        let user_apps = home.join("Applications/Minecraft.app/Contents/MacOS/launcher");
        if user_apps.exists() {
            debug!("Found Minecraft launcher in user Applications: {}", user_apps.display());
            return Some(user_apps);
        }
    }
    
    warn!("Could not find Minecraft launcher on macOS");
    None
}

/// Linux implementation
#[cfg(target_os = "linux")]
pub fn find_minecraft_launcher() -> Option<PathBuf> {
    let search_paths = vec![
        "/usr/bin/minecraft-launcher",
        "/usr/local/bin/minecraft-launcher",
        "/opt/minecraft-launcher/minecraft-launcher",
        "/snap/bin/minecraft-launcher",  // Snap package
        "/var/lib/flatpak/exports/bin/com.mojang.Minecraft",  // Flatpak
    ];
    
    for path_str in &search_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            debug!("Found Minecraft launcher at: {}", path.display());
            return Some(path);
        }
    }
    
    // Check user's local bin
    if let Some(home) = dirs::home_dir() {
        let local_paths = vec![
            home.join(".local/bin/minecraft-launcher"),
            home.join("bin/minecraft-launcher"),
            home.join(".local/share/applications/minecraft-launcher"),
        ];
        
        for path in local_paths {
            if path.exists() {
                debug!("Found Minecraft launcher in user directory: {}", path.display());
                return Some(path);
            }
        }
    }
    
    warn!("Could not find Minecraft launcher on Linux");
    None
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmcLikeLauncher {
    MultiMC,
    Prism,
}

impl MmcLikeLauncher {
    fn exe_names(&self) -> (&'static str, &'static str, &'static str) {
        // (windows, macos-app-relative-binary, linux)
        match self {
            MmcLikeLauncher::MultiMC => ("MultiMC.exe", "MultiMC.app/Contents/MacOS/MultiMC", "MultiMC"),
            MmcLikeLauncher::Prism => (
                "prismlauncher.exe",
                "Prism Launcher.app/Contents/MacOS/prismlauncher",
                "prismlauncher",
            ),
        }
    }

    fn flatpak_id(&self) -> Option<&'static str> {
        match self {
            MmcLikeLauncher::MultiMC => None, // no official Flatpak
            MmcLikeLauncher::Prism => Some("org.prismlauncher.PrismLauncher"),
        }
    }

    fn cache_file_name(&self) -> &'static str {
        match self {
            MmcLikeLauncher::MultiMC => "multimc_path.txt",
            MmcLikeLauncher::Prism => "prism_path.txt",
        }
    }
}

/// How to actually invoke the launcher: a direct executable, or `flatpak run <id>`.
#[derive(Debug, Clone)]
pub enum MmcLikeCommand {
    Direct(PathBuf),
    Flatpak(String),
}

/// Locate the actual MultiMC/Prism *executable* (not its data directory —
/// that's what `crate::get_multimc_folder` returns, and it is frequently a
/// completely different path from where the binary lives, e.g. Flatpak,
/// Program Files, /usr/bin, a Snap, or a Microsoft Store install).
pub fn find_mmc_like_launcher(launcher: MmcLikeLauncher) -> Option<MmcLikeCommand> {
    if let Some(cached) = load_cached_mmc_path(launcher) {
        match cached {
            MmcLikeCommand::Direct(ref p) if p.exists() => return Some(cached),
            MmcLikeCommand::Flatpak(_) => return Some(cached),
            _ => warn!("Cached {:?} path no longer exists, searching again...", launcher),
        }
    }

    #[allow(unused_variables)]
    let (win_name, mac_rel, linux_name) = launcher.exe_names();

    #[cfg(target_os = "windows")]
    {
        let candidates = [
            PathBuf::from(format!("C:\\Program Files\\{}", match launcher {
                MmcLikeLauncher::MultiMC => "MultiMC",
                MmcLikeLauncher::Prism => "Prism Launcher",
            })).join(win_name),
            PathBuf::from(format!("C:\\Program Files (x86)\\{}", match launcher {
                MmcLikeLauncher::MultiMC => "MultiMC",
                MmcLikeLauncher::Prism => "Prism Launcher",
            })).join(win_name),
        ];
        for path in &candidates {
            if path.exists() {
                let cmd = MmcLikeCommand::Direct(path.clone());
                cache_mmc_path(launcher, &cmd);
                return Some(cmd);
            }
        }
        // Common portable installs: sitting right next to the data dir.
        if let Ok(data_dir) = crate::get_multimc_folder(match launcher {
            MmcLikeLauncher::MultiMC => "MultiMC",
            MmcLikeLauncher::Prism => "PrismLauncher",
        }) {
            let portable = data_dir.join(win_name);
            if portable.exists() {
                let cmd = MmcLikeCommand::Direct(portable);
                cache_mmc_path(launcher, &cmd);
                return Some(cmd);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            PathBuf::from("/Applications").join(mac_rel),
        ];
        for path in &candidates {
            if path.exists() {
                let cmd = MmcLikeCommand::Direct(path.clone());
                cache_mmc_path(launcher, &cmd);
                return Some(cmd);
            }
        }
        if let Some(home) = dirs::home_dir() {
            let user_apps = home.join("Applications").join(mac_rel);
            if user_apps.exists() {
                let cmd = MmcLikeCommand::Direct(user_apps);
                cache_mmc_path(launcher, &cmd);
                return Some(cmd);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            PathBuf::from("/usr/bin").join(linux_name),
            PathBuf::from("/usr/local/bin").join(linux_name),
            PathBuf::from("/opt").join(linux_name).join(linux_name),
            PathBuf::from("/var/lib/snapd/snap/bin").join(linux_name),
        ];
        for path in &candidates {
            if path.exists() {
                let cmd = MmcLikeCommand::Direct(path.clone());
                cache_mmc_path(launcher, &cmd);
                return Some(cmd);
            }
        }
        if let Some(home) = dirs::home_dir() {
            let user_candidates = [
                home.join(".local/bin").join(linux_name),
                home.join(".local/share/flatpak/exports/bin").join(
                    launcher.flatpak_id().unwrap_or_default(),
                ),
            ];
            for path in &user_candidates {
                if !path.as_os_str().is_empty() && path.exists() {
                    let cmd = if path.to_string_lossy().contains("flatpak") {
                        MmcLikeCommand::Flatpak(launcher.flatpak_id().unwrap().to_string())
                    } else {
                        MmcLikeCommand::Direct(path.clone())
                    };
                    cache_mmc_path(launcher, &cmd);
                    return Some(cmd);
                }
            }
        }
        // System-wide Flatpak export
        if let Some(flatpak_id) = launcher.flatpak_id() {
            let system_export = PathBuf::from("/var/lib/flatpak/exports/bin").join(flatpak_id);
            if system_export.exists() {
                let cmd = MmcLikeCommand::Flatpak(flatpak_id.to_string());
                cache_mmc_path(launcher, &cmd);
                return Some(cmd);
            }
        }
        // Portable install sitting in the data dir (rare on Linux, but cheap to check).
        if let Ok(data_dir) = crate::get_multimc_folder(match launcher {
            MmcLikeLauncher::MultiMC => "MultiMC",
            MmcLikeLauncher::Prism => "PrismLauncher",
        }) {
            let portable = data_dir.join(linux_name);
            if portable.exists() {
                let cmd = MmcLikeCommand::Direct(portable);
                cache_mmc_path(launcher, &cmd);
                return Some(cmd);
            }
        }
    }

    warn!("Could not find {:?} executable in any known location", launcher);
    None
}

fn cache_mmc_path(launcher: MmcLikeLauncher, cmd: &MmcLikeCommand) {
    let cache_file = crate::get_app_data().join(".WC_OVHL").join(launcher.cache_file_name());
    let contents = match cmd {
        MmcLikeCommand::Direct(p) => format!("direct:{}", p.to_string_lossy()),
        MmcLikeCommand::Flatpak(id) => format!("flatpak:{}", id),
    };
    if let Err(e) = std::fs::write(&cache_file, contents) {
        warn!("Failed to cache {:?} path: {}", launcher, e);
    }
}

fn load_cached_mmc_path(launcher: MmcLikeLauncher) -> Option<MmcLikeCommand> {
    let cache_file = crate::get_app_data().join(".WC_OVHL").join(launcher.cache_file_name());
    let content = std::fs::read_to_string(&cache_file).ok()?;
    let content = content.trim();
    if let Some(rest) = content.strip_prefix("direct:") {
        Some(MmcLikeCommand::Direct(PathBuf::from(rest)))
    } else { content.strip_prefix("flatpak:").map(|rest| MmcLikeCommand::Flatpak(rest.to_string())) }
}

/// Get a cached or freshly searched launcher path
pub fn get_launcher_path() -> Result<PathBuf, String> {
    // Try to load from cache first
    if let Some(cached_path) = load_cached_launcher_path() {
        if cached_path.exists() {
            debug!("Using cached launcher path: {}", cached_path.display());
            return Ok(cached_path);
        } else {
            warn!("Cached launcher path no longer exists, searching again...");
        }
    }
    
    // Search for the launcher
    match find_minecraft_launcher() {
        Some(path) => {
            info!("Found Minecraft launcher at: {}", path.display());
            // Cache the path for future use
            save_launcher_path_cache(&path);
            Ok(path)
        },
        None => Err("Could not find Minecraft launcher. Please ensure Minecraft is installed, or manually launch it from the Minecraft launcher.".to_string())
    }
}

/// Save the launcher path to cache
fn save_launcher_path_cache(path: &Path) {
    let cache_file = crate::get_app_data().join(".WC_OVHL/launcher_path.txt");
    if let Err(e) = std::fs::write(&cache_file, path.to_string_lossy().as_bytes()) {
        warn!("Failed to cache launcher path: {}", e);
    }
}

/// Load the cached launcher path
fn load_cached_launcher_path() -> Option<PathBuf> {
    let cache_file = crate::get_app_data().join(".WC_OVHL/launcher_path.txt");
    if let Ok(content) = std::fs::read_to_string(&cache_file) {
        let path = PathBuf::from(content.trim());
        Some(path)
    } else {
        None
    }
}
