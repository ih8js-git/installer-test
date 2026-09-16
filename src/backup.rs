use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Write};
use chrono::{DateTime, Utc};
use log::{debug, warn};
use zip::{ZipWriter, CompressionMethod};

/// Enhanced backup item discovery with better file/folder scanning
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupItem {
    pub name: String,
    pub path: PathBuf,
    pub is_directory: bool,
    pub size_bytes: u64,
    pub file_count: Option<usize>, // Only for directories
    pub description: Option<String>, // User-friendly description
    pub children: Option<Vec<BackupItem>>, // NEW: For nested items
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileSystemItem {
    pub name: String,
    pub path: PathBuf,
    pub is_directory: bool,
    pub size_bytes: u64,
    pub file_count: Option<usize>, // For directories
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    pub children: Option<Vec<FileSystemItem>>, // For tree view
    pub is_selected: bool, // User selection state
}

/// Progress tracking for backup operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupProgress {
    pub current_file: String,
    pub files_processed: usize,
    pub total_files: usize,
    pub bytes_processed: u64,
    pub total_bytes: u64,
    pub current_operation: String, // e.g., "Scanning files", "Creating archive"
}

/// Configuration for what to include in backups
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupConfig {
    pub selected_items: Vec<String>, // Paths relative to installation root
    pub compress_backups: bool,
    pub max_backups: usize,
    pub include_hidden_files: bool,
    pub exclude_patterns: Vec<String>, // Glob patterns to exclude
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            selected_items: vec![
                "mods".to_string(),
                "config".to_string(),
                "resourcepacks".to_string(),
                "shaderpacks".to_string(),
            ],
            compress_backups: true,
            max_backups: 10,
            include_hidden_files: false,
            exclude_patterns: vec![
                "*.log".to_string(),
                "crash-reports".to_string(),
                "logs".to_string(),
                "temp".to_string(),
                "cache".to_string(),
            ],
        }
    }
}

/// Types of backups that can be created
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackupType {
    Manual,
    PreUpdate,
    PreInstall,
    Scheduled,
}

/// Metadata about a backup
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupMetadata {
    pub id: String,
    pub description: String,
    pub backup_type: BackupType,
    pub created_at: DateTime<Utc>,
    pub modpack_version: String,
    pub enabled_features: Vec<String>,
    pub file_count: usize,
    pub size_bytes: u64,
    pub included_items: Vec<String>, // List of backed up items
    pub config: BackupConfig,
}

impl FileSystemItem {
    // Add this method that was referenced but missing
    pub fn scan_directory(root_path: &Path, max_depth: usize) -> Result<Vec<FileSystemItem>, String> {
        Self::scan_installation_with_depth(root_path, max_depth)
    }
    
    /// Scan a directory and build a tree of all files and folders
    pub fn scan_installation(root_path: &Path) -> Result<Vec<FileSystemItem>, String> {
        Self::scan_installation_with_depth(root_path, 5)
    }
    
    pub fn scan_installation_with_depth(root_path: &Path, max_depth: usize) -> Result<Vec<FileSystemItem>, String> {
        debug!("Scanning installation directory: {:?}", root_path);
        
        if !root_path.exists() {
            return Err("Installation directory does not exist".to_string());
        }
        
        Self::scan_directory_recursive(root_path, root_path, 0, max_depth)
    }
    
    fn scan_directory_recursive(
        root_path: &Path,
        current_path: &Path,
        current_depth: usize,
        max_depth: usize
    ) -> Result<Vec<FileSystemItem>, String> {
        if current_depth > max_depth {
            return Ok(Vec::new());
        }
        
        let mut items = Vec::new();
        
        let entries = fs::read_dir(current_path)
            .map_err(|e| format!("Failed to read directory {:?}: {}", current_path, e))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            
            // Skip system files and backup-related temp files
            if Self::should_skip_item(&name) {
                continue;
            }
            
            let metadata = path.metadata()
                .map_err(|e| format!("Failed to get metadata for {:?}: {}", path, e))?;
            
            let is_directory = metadata.is_dir();
            let last_modified = metadata.modified().ok()
                .and_then(|time| chrono::DateTime::from_timestamp(
                    time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64, 0
                ));
            
            // Get relative path from root
            let relative_path = path.strip_prefix(root_path)
                .unwrap_or(&path)
                .to_path_buf();
            
            let (size_bytes, file_count, children) = if is_directory {
                let dir_size = calculate_directory_size(&path).unwrap_or(0);
                let dir_file_count = count_files_recursive(&path).unwrap_or(0);
                
                // Recursively scan children
                let dir_children = if current_depth < max_depth {
                    Self::scan_directory_recursive(root_path, &path, current_depth + 1, max_depth)
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                
                (dir_size, Some(dir_file_count), Some(dir_children))
            } else {
                (metadata.len(), None, None)
            };
            
            items.push(FileSystemItem {
                name,
                path: relative_path,
                is_directory,
                size_bytes,
                file_count,
                last_modified,
                children,
                is_selected: Self::is_default_selected(&path),
            });
        }
        
        // Sort: directories first, then files, both alphabetically
        items.sort_by(|a, b| {
            match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        
        Ok(items)
    }
    
    fn should_skip_item(name: &str) -> bool {
        // Skip system files, temp files, and backup-related files
        let skip_patterns = [
            ".DS_Store", "Thumbs.db", "desktop.ini",
            "backup.zip", "tmp_include.zip", ".tmp",
            "manifest.json", "launcher_profiles.json",
            "backups", // Don't include the backups folder itself
        ];
        
        skip_patterns.iter().any(|pattern| {
            name == *pattern || name.starts_with("tmp_") || name.ends_with(".tmp")
        })
    }
    
    fn is_default_selected(path: &Path) -> bool {
        // Auto-select commonly important folders
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            matches!(name, "mods" | "config" | "resourcepacks" | "shaderpacks" | "wynntils")
        } else {
            false
        }
    }
    
    /// Get all selected paths recursively
    pub fn get_selected_paths(&self) -> Vec<PathBuf> {
        let mut selected = Vec::new();
        
        if self.is_selected {
            selected.push(self.path.clone());
            // If a directory is selected, we include all its contents
            // so no need to check children
        } else if let Some(children) = &self.children {
            // If directory not selected, check children
            for child in children {
                selected.extend(child.get_selected_paths());
            }
        }
        
        selected
    }
    
    /// Toggle selection state for a specific path
    pub fn toggle_selection(&mut self, path: &Path) {
        if self.path == path {
            self.is_selected = !self.is_selected;
            // If selecting a directory, auto-select all children
            if self.is_selected && self.is_directory {
                self.set_all_children_selected(true);
            } else if !self.is_selected && self.is_directory {
                self.set_all_children_selected(false);
            }
        } else if let Some(children) = &mut self.children {
            for child in children {
                child.toggle_selection(path);
            }
        }
    }
    
    pub fn set_all_children_selected(&mut self, selected: bool) {
        self.is_selected = selected;
        if let Some(children) = &mut self.children {
            for child in children {
                child.set_all_children_selected(selected);
            }
        }
    }
}


impl BackupMetadata {
    pub fn age_description(&self) -> String {
        let now = Utc::now();
        let duration = now.signed_duration_since(self.created_at);
        
        if duration.num_days() > 0 {
            format!("{} days ago", duration.num_days())
        } else if duration.num_hours() > 0 {
            format!("{} hours ago", duration.num_hours())
        } else if duration.num_minutes() > 0 {
            format!("{} minutes ago", duration.num_minutes())
        } else {
            "Just now".to_string()
        }
    }
    
    pub fn formatted_size(&self) -> String {
        format_bytes(self.size_bytes)
    }
}

/// Rollback manager for emergency recovery
#[derive(Debug, Clone)]
pub struct RollbackManager {
    pub installation: crate::installation::Installation,
}

impl RollbackManager {
    pub fn new(installation: crate::installation::Installation) -> Self {
        Self { installation }
    }
    
    pub fn get_rollback_options(&self) -> Result<Vec<RollbackOption>, String> {
        let backups = self.installation.list_available_backups()?;
        let mut options = Vec::new();
        
        for backup in &backups {
            let is_recommended = backup.backup_type == BackupType::PreUpdate || 
                               backup.backup_type == BackupType::Manual;
            
            options.push(RollbackOption {
                backup_id: backup.id.clone(),
                description: backup.description.clone(),
                modpack_version: backup.modpack_version.clone(),
                created_at: backup.created_at,
                size: backup.size_bytes,
                is_recommended,
            });
        }
        
        options.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(options)
    }
    
    pub async fn rollback_to_last_working(&mut self) -> Result<(), String> {
        let options = self.get_rollback_options()?;
        
        if let Some(last_working) = options.first() {
            self.rollback_to_backup(&last_working.backup_id).await
        } else {
            Err("No backups available for rollback".to_string())
        }
    }
    
    pub async fn rollback_to_backup(&mut self, backup_id: &str) -> Result<(), String> {
        let safety_config = BackupConfig::default();
        let safety_description = format!("Safety backup before rollback to {}", backup_id);
        
        self.installation.create_backup(
            BackupType::PreUpdate,
            &safety_config,
            safety_description,
            None::<fn(BackupProgress)>,
        ).await?;
        
        self.installation.restore_from_backup(backup_id).await?;
        Ok(())
    }
}

/// Rollback option for the UI
#[derive(Debug, Clone, PartialEq)]
pub struct RollbackOption {
    pub backup_id: String,
    pub description: String,
    pub modpack_version: String,
    pub created_at: DateTime<Utc>,
    pub size: u64,
    pub is_recommended: bool,
}

/// Format bytes in a human-readable way
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    
    if bytes == 0 {
        return "0 B".to_string();
    }
    
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Create a ZIP archive from a directory
pub fn create_zip_archive<F>(
    source_dir: &std::path::Path,
    zip_path: &std::path::Path,
    progress_callback: Option<&F>,
) -> Result<u64, String>
where
    F: Fn(BackupProgress),
{
    let file = fs::File::create(zip_path)
        .map_err(|e| format!("Failed to create zip file: {}", e))?;
    let mut zip = ZipWriter::new(file);
    
    let total_files = count_files_recursive(source_dir)
        .map_err(|e| format!("Failed to count files: {}", e))?;
    let mut files_processed = 0;
    let mut bytes_processed = 0;
    
    add_directory_to_zip(
        &mut zip,
        source_dir,
        "",
        &mut files_processed,
        total_files,
        &mut bytes_processed,
        &progress_callback,
    ).map_err(|e| format!("Failed to add directory to zip: {}", e))?;
    
    let final_archive = zip.finish()
        .map_err(|e| format!("Failed to finish zip archive: {}", e))?;
    let final_size = final_archive.metadata()
        .map_err(|e| format!("Failed to get archive metadata: {}", e))?
        .len();
    Ok(final_size)
}

fn add_directory_to_zip<F, W: Write + io::Seek>(
    zip: &mut ZipWriter<W>,
    source_dir: &std::path::Path,
    prefix: &str,
    files_processed: &mut usize,
    total_files: usize,
    bytes_processed: &mut u64,
    progress_callback: &Option<&F>,
) -> Result<(), io::Error>
where
    F: Fn(BackupProgress),
{
    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        let full_name = if prefix.is_empty() {
            name_str.to_string()
        } else {
            format!("{}/{}", prefix, name_str)
        };
        
        if path.is_file() {
            let file_data = fs::read(&path)?;
            let file_size = file_data.len() as u64;
            
            let options = zip::write::FileOptions::<()>::default()
                .compression_method(CompressionMethod::Deflated);
            zip.start_file(&full_name, options)?;
            zip.write_all(&file_data)?;
            
            *files_processed += 1;
            *bytes_processed += file_size;
            
            if let Some(callback) = progress_callback {
                callback(BackupProgress {
                    current_file: full_name,
                    files_processed: *files_processed,
                    total_files,
                    bytes_processed: *bytes_processed,
                    total_bytes: 0,
                    current_operation: "Compressing files".to_string(),
                });
            }
        } else if path.is_dir() {
            add_directory_to_zip(
                zip,
                &path,
                &full_name,
                files_processed,
                total_files,
                bytes_processed,
                progress_callback,
            )?;
        }
    }
    
    Ok(())
}

/// Extract a ZIP archive to a directory
pub fn extract_zip_archive(zip_path: &std::path::Path, destination: &std::path::Path) -> Result<(), io::Error> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = destination.join(file.name());
        
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            
            let mut outfile = fs::File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }
    }
    
    Ok(())
}

impl BackupItem {
    /// Create a new backup item with enhanced metadata
    pub fn new(
        name: String,
        path: PathBuf,
        is_directory: bool,
        include_children: bool,
    ) -> Result<Self, String> {
        let full_path = path.clone();
        
        let (size_bytes, file_count) = if is_directory {
            let size = calculate_directory_size(&full_path)?;
            let count = count_files_recursive(&full_path)
                .map_err(|e| format!("Failed to count files: {}", e))?;
            (size, Some(count))
        } else {
            let size = fs::metadata(&full_path)
                .map_err(|e| format!("Failed to get file metadata: {}", e))?
                .len();
            (size, None)
        };
        
        let description = get_item_description(&name, is_directory);
        
        let children = if include_children && is_directory {
            Some(discover_children(&full_path)?)
        } else {
            None
        };
        
        Ok(BackupItem {
            name,
            path,
            is_directory,
            size_bytes,
            file_count,
            description,
            children,
        })
    }
    
    /// Get a flat list of all items including children
    pub fn flatten(&self) -> Vec<BackupItem> {
        let mut items = vec![self.clone()];
        
        if let Some(children) = &self.children {
            for child in children {
                items.extend(child.flatten());
            }
        }
        
        items
    }
    
    /// Check if this item should be included in backup based on patterns
    pub fn should_include(&self, include_patterns: &[String], exclude_patterns: &[String]) -> bool {
        let path_str = self.path.to_string_lossy().to_string();
        let name = &self.name;
        
        // Check exclude patterns first
        for pattern in exclude_patterns {
            if matches_pattern(&path_str, pattern) || matches_pattern(name, pattern) {
                return false;
            }
        }
        
        // If no include patterns specified, include everything not excluded
        if include_patterns.is_empty() {
            return true;
        }
        
        // Check include patterns
        for pattern in include_patterns {
            if matches_pattern(&path_str, pattern) || matches_pattern(name, pattern) {
                return true;
            }
        }
        
        false
    }
}

/// Enhanced backup discovery function
pub fn discover_installation_items(installation_path: &Path, max_depth: usize) -> Result<Vec<BackupItem>, String> {
    let mut items = Vec::new();
    
    debug!("Discovering backup items in: {:?}", installation_path);
    
    if !installation_path.exists() {
        return Err("Installation path does not exist".to_string());
    }
    
    let entries = fs::read_dir(installation_path)
        .map_err(|e| format!("Failed to read installation directory: {}", e))?;
    
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        
        // Skip hidden files unless specifically allowed
        if name.starts_with('.') && !is_allowed_hidden_item(&name) {
            continue;
        }
        
        // Skip known temporary/system files
        if should_skip_item(&name) {
            continue;
        }
        
        let relative_path = path.strip_prefix(installation_path)
            .unwrap_or(&path)
            .to_path_buf();
        
        let is_directory = path.is_dir();
        
        match BackupItem::new(name, relative_path, is_directory, max_depth > 0) {
            Ok(item) => {
                debug!("Discovered item: {} ({})", item.name, 
                       if item.is_directory { "directory" } else { "file" });
                items.push(item);
            },
            Err(e) => {
                warn!("Failed to create backup item for {:?}: {}", path, e);
            }
        }
    }
    
    // Sort by priority and name
    items.sort_by(|a, b| {
        let a_priority = get_folder_priority(&a.name);
        let b_priority = get_folder_priority(&b.name);
        
        match a_priority.cmp(&b_priority) {
            std::cmp::Ordering::Equal => a.name.cmp(&b.name),
            other => other,
        }
    });
    
    debug!("Discovered {} backup items", items.len());
    Ok(items)
}

/// Discover children of a directory (one level deep)
fn discover_children(dir_path: &Path) -> Result<Vec<BackupItem>, String> {
    let mut children = Vec::new();
    
    if !dir_path.is_dir() {
        return Ok(children);
    }
    
    let entries = fs::read_dir(dir_path)
        .map_err(|e| format!("Failed to read directory: {}", e))?;
    
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        
        if should_skip_item(&name) {
            continue;
        }
        
        let is_directory = path.is_dir();
        
        match BackupItem::new(name, path, is_directory, false) {
            Ok(item) => children.push(item),
            Err(e) => warn!("Failed to create child item: {}", e),
        }
    }
    
    // Limit children to prevent UI overload
    children.sort_by(|a, b| a.name.cmp(&b.name));
    children.truncate(50); // Limit to 50 children per directory
    
    Ok(children)
}

// The backup creation implementation will be defined in installation.rs to avoid circular dependencies
// This is just the interface definition

// Helper functions

fn get_item_description(name: &str, is_directory: bool) -> Option<String> {
    let description = match name.to_lowercase().as_str() {
        "mods" => "Mod files and configurations",
        "config" => "Game and mod configuration files",
        "resourcepacks" => "Resource pack files",
        "shaderpacks" => "Shader pack files",
        "saves" => "World save files",
        "screenshots" => "Screenshot images",
        "logs" => "Game and launcher log files",
        "crash-reports" => "Crash report files",
        "wynntils" => "Wynntils mod configuration and data",
        "options.txt" => "Minecraft game options",
        "servers.dat" => "Multiplayer server list",
        "usercache.json" => "User cache data",
        "usernamecache.json" => "Username cache data",
        _ => {
            if is_directory {
                "Custom directory"
            } else if name.ends_with(".json") {
                "Configuration file"
            } else if name.ends_with(".txt") {
                "Text file"
            } else if name.ends_with(".jar") {
                "Java application file"
            } else {
                "Custom file"
            }
        }
    };
    
    Some(description.to_string())
}

fn get_folder_priority(name: &str) -> u8 {
    match name.to_lowercase().as_str() {
        "mods" => 1,
        "config" => 2,
        "saves" => 3,
        "resourcepacks" => 4,
        "shaderpacks" => 5,
        "wynntils" => 6,
        "screenshots" => 7,
        "logs" => 8,
        "crash-reports" => 9,
        _ => 10,
    }
}

fn should_skip_item(name: &str) -> bool {
    let skip_items = [
        "manifest.json", "launcher_profiles.json",
        "usernamecache.json", "usercache.json",
        "backup.zip", "tmp_include.zip",
        ".DS_Store", "Thumbs.db", "desktop.ini",
    ];
    
    skip_items.contains(&name) || name.starts_with("tmp_") || name.ends_with(".tmp")
}

fn is_allowed_hidden_item(name: &str) -> bool {
    matches!(name, ".minecraft" | ".fabric" | ".quilt")
}

fn should_exclude_path(path: &Path, exclude_patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    
    for pattern in exclude_patterns {
        if matches_pattern(&path_str, pattern) || matches_pattern(&file_name, pattern) {
            return true;
        }
    }
    
    false
}

fn matches_pattern(text: &str, pattern: &str) -> bool {
    if pattern.contains('*') {
        // Simple glob matching
        let pattern_parts: Vec<&str> = pattern.split('*').collect();
        if pattern_parts.len() == 2 {
            let prefix = pattern_parts[0];
            let suffix = pattern_parts[1];
            text.starts_with(prefix) && text.ends_with(suffix)
        } else {
            // More complex patterns - just check if any part matches
            pattern_parts.iter().any(|part| !part.is_empty() && text.contains(part))
        }
    } else {
        
        text == pattern || text.ends_with(pattern)
    }
}

pub fn calculate_directory_size(path: &Path) -> Result<u64, String> {
    let mut total_size = 0;
    
    if path.is_file() {
        return Ok(path.metadata()
            .map_err(|e| format!("Failed to get file metadata: {}", e))?
            .len());
    }
    
    if path.is_dir() {
        let entries = fs::read_dir(path)
            .map_err(|e| format!("Failed to read directory: {}", e))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let entry_path = entry.path();
            
            if entry_path.is_file() {
                total_size += entry.metadata()
                    .map_err(|e| format!("Failed to get entry metadata: {}", e))?
                    .len();
            } else if entry_path.is_dir() {
                total_size += calculate_directory_size(&entry_path)?;
            }
        }
    }
    
    Ok(total_size)
}

pub fn count_files_recursive(path: &Path) -> Result<usize, io::Error> {
    let mut count = 0;
    
    if path.is_file() {
        return Ok(1);
    }
    
    if path.is_dir() {
        let entries = fs::read_dir(path)?;
        
        for entry in entries {
            let entry = entry?;
            let entry_path = entry.path();
            
            if entry_path.is_file() {
                count += 1;
            } else if entry_path.is_dir() {
                count += count_files_recursive(&entry_path)?;
            }
        }
    }
    
    Ok(count)
}
