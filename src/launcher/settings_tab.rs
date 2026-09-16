use dioxus::prelude::*;
use crate::installation::Installation;
use crate::backup::{BackupConfig, BackupType, BackupMetadata, BackupProgress};
use log::{debug, error, warn}; // Only import from log, remove the duplicate

#[component]
pub fn SettingsTab(
    installation: Installation,
    installation_id: String,
    ondelete: EventHandler<()>,
    onupdate: EventHandler<Installation>,
) -> Element {
    // Remove the tab navigation - everything goes in one page now
    rsx! {
        div { class: "settings-tab",
            h2 { "Installation Settings" }
            
            GeneralSettingsSection {
                installation: installation.clone(),
                installation_id: installation_id.clone(),
                ondelete: ondelete,
                onupdate: onupdate
            }
        }
    }
}

#[component]
fn GeneralSettingsSection(
    installation: Installation,
    installation_id: String,
    ondelete: EventHandler<()>,
    onupdate: EventHandler<Installation>,
) -> Element {
    // Clone everything we need to use across closures
    let installation_path_display = installation.installation_path.display().to_string();
    let installation_name = installation.name.clone();
    let installation_id_for_delete = installation_id.clone();
    let installation_id_for_cache = installation_id.clone();
    
    // Clone values needed in the UI (outside of closures)
    let _path_for_ui = installation.installation_path.clone();
    let minecraft_version = installation.minecraft_version.clone();
    let loader_type = installation.loader_type.clone();
    let loader_version = installation.loader_version.clone();
    let launcher_type = installation.launcher_type.clone();
    let created_at = installation.created_at;
    let last_used = installation.last_used;
    let total_launches = installation.total_launches;
    let last_launch = installation.last_launch;
    
    // State for rename dialog
    let mut show_rename_dialog = use_signal(|| false);
    let mut new_name = use_signal(|| installation_name.clone());
    let mut rename_error = use_signal(|| Option::<String>::None);
    
    // State for delete confirmation
    let mut show_delete_confirm = use_signal(|| false);
    
    // State for operation status
    let mut operation_error = use_signal(|| Option::<String>::None);
    let mut is_operating = use_signal(|| false);
    
    // Backup-related state
    let mut show_backup_section = use_signal(|| false);
    let mut backup_config = use_signal(BackupConfig::default);
    let mut backup_description = use_signal(String::new);
    let available_backups = use_signal(Vec::<BackupMetadata>::new);
    let mut selected_backup = use_signal(|| Option::<String>::None);
    let is_creating_backup = use_signal(|| false);
    let backup_progress = use_signal(|| None::<BackupProgress>);
    let mut backup_success = use_signal(|| Option::<String>::None);
    let mut show_backup_config = use_signal(|| false);
    let mut show_restore_confirm = use_signal(|| false);
    
    // Load available backups when backup section is shown
    use_effect({
        let installation_clone = installation.clone();
        let mut available_backups = available_backups;
        let show_backup_section = show_backup_section;
        
        move || {
            if *show_backup_section.read() {
                match installation_clone.list_available_backups() {
                    Ok(backups) => {
                        debug!("Loaded {} backups for installation", backups.len());
                        available_backups.set(backups);
                    },
                    Err(e) => {
                        error!("Failed to load backups: {}", e);
                    }
                }
            }
        }
    });
    
    // Open folder function
    let installation_path_for_folder = installation.installation_path.clone();
    let open_folder = move |_| {
        let path = installation_path_for_folder.clone();
        
        debug!("Opening installation folder: {:?}", path);
        
        // Launch appropriate command based on OS
        #[cfg(target_os = "windows")]
        let result = {
            let path_str = path.to_string_lossy().replace("/", "\\");
            std::process::Command::new("explorer")
                .arg(&path_str)
                .spawn()
        };
        
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open")
            .arg(path)
            .spawn();
            
        #[cfg(target_os = "linux")]
        let result = std::process::Command::new("xdg-open")
            .arg(path)
            .spawn();
            
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        let result = Err(std::io::Error::new(std::io::ErrorKind::Other, "Unsupported platform"));
        
        if let Err(e) = result {
            debug!("Failed to open installation folder: {}", e);
            operation_error.set(Some(format!("Failed to open folder: {}", e)));
        }
    };
    
    // Handle rename
    let installation_for_rename = installation.clone();
    let handle_rename = move |_| {
        let mut installation_copy = installation_for_rename.clone();
        let new_name_trimmed = new_name.read().trim().to_string();
        installation_copy.name = new_name_trimmed.clone();
        
        const MAX_NAME_LENGTH: usize = 25;
        if new_name_trimmed.is_empty() {
            rename_error.set(Some("Installation name cannot be empty".to_string()));
            return;
        }
        
        if new_name_trimmed.len() > MAX_NAME_LENGTH {
            rename_error.set(Some(format!("Installation name cannot exceed {} characters.", MAX_NAME_LENGTH)));
            return;
        }
        
        is_operating.set(true);
        
        match installation_copy.save() {
            Ok(_) => {
                debug!("Renamed installation to: {}", installation_copy.name);
                show_rename_dialog.set(false);
                is_operating.set(false);
                rename_error.set(None);
                onupdate.call(installation_copy);
            },
            Err(e) => {
                debug!("Failed to rename installation: {}", e);
                rename_error.set(Some(format!("Failed to rename installation: {}", e)));
                is_operating.set(false);
            }
        }
    };
    
    // Handle delete
    let handle_delete = move |_| {
        let id_to_delete = installation_id_for_delete.clone();
        let delete_handler = ondelete;
        let mut operation_error_clone = operation_error;
        let mut is_operating_clone = is_operating;
        
        is_operating_clone.set(true);
        
        spawn(async move {
            match crate::installation::delete_installation(&id_to_delete) {
                Ok(_) => {
                    debug!("Successfully deleted installation: {}", id_to_delete);
                    delete_handler.call(());
                },
                Err(e) => {
                    error!("Failed to delete installation: {}", e);
                    operation_error_clone.set(Some(format!("Failed to delete installation: {}", e)));
                    is_operating_clone.set(false);
                }
            }
        });
    };
    
    // Backup functions
    let create_backup = {
        let installation_clone = installation.clone();
        let mut is_creating_backup = is_creating_backup;
        let mut backup_progress = backup_progress;
        let mut operation_error = operation_error;
        let mut backup_success = backup_success;
        let backup_config = backup_config;
        let backup_description = backup_description;
        let mut available_backups = available_backups;
        
        move |_| {
            let installation = installation_clone.clone();
            let config = backup_config.read().clone();
            let description = backup_description.read().clone();
            let description = if description.trim().is_empty() {
                format!("Manual backup - {}", chrono::Utc::now().format("%Y-%m-%d %H:%M"))
            } else {
                description
            };
            
            is_creating_backup.set(true);
            backup_progress.set(None);
            operation_error.set(None);
            backup_success.set(None);
            
            spawn(async move {
                let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<BackupProgress>();
                
                let progress_callback = move |progress: BackupProgress| {
                    let _ = progress_tx.send(progress);
                };
                
                let mut backup_progress_clone = backup_progress;
                spawn(async move {
                    while let Some(progress) = progress_rx.recv().await {
                        backup_progress_clone.set(Some(progress));
                    }
                });
                
                match installation.create_backup(
                    BackupType::Manual,
                    &config,
                    description.clone(),
                    Some(progress_callback),
                ).await {
                    Ok(metadata) => {
                        backup_success.set(Some(format!("Backup created successfully: {}", metadata.id)));
                        
                        if let Ok(backups) = installation.list_available_backups() {
                            available_backups.set(backups);
                        }
                    },
                    Err(e) => {
                        operation_error.set(Some(format!("Failed to create backup: {}", e)));
                    }
                }
                
                is_creating_backup.set(false);
                backup_progress.set(None);
            });
        }
    };
    
rsx! {
    div { class: "settings-tab",
        // Display operation error if any
        {operation_error.read().as_ref().map(|error| rsx! {
            div { class: "error-notification settings-error",
                div { class: "error-message", "{error}" }
                button { 
                    class: "error-close",
                    onclick: move |_| operation_error.set(None),
                    "×"
                }
            }
        })}
        
        // Success message for backups
        {backup_success.read().as_ref().map(|success| rsx! {
            div { class: "success-notification",
                div { class: "success-message", "{success}" }
                button { 
                    class: "success-close",
                    onclick: move |_| backup_success.set(None),
                    "×"
                }
            }
        })}
        
        // Installation information section
        div { class: "settings-section installation-info",
            h3 { "Installation Information" }
            
            div { class: "info-grid",
                div { class: "info-row",
                    div { class: "info-label", "Name: ", "{installation_name}" }
                }
                
                div { class: "info-row",
                    div { class: "info-label", "Created: ", "{created_at.format(\"%B %d, %Y\")}" }
                }
                
                div { class: "info-row",
                    div { class: "info-label", "Last Used: ", "{last_used.format(\"%B %d, %Y\")}"  }
                }
                
                div { class: "info-row",
                    div { class: "info-label", "Minecraft: ", "{minecraft_version}" }
                }
                
                div { class: "info-row",
                    div { class: "info-label", "Loader: ", "{loader_type} {loader_version}"  }
                }
                
                div { class: "info-row",
                    div { class: "info-label", "Launcher: ", "{launcher_type}" }
                }
                
                div { class: "info-row path-row",
                    div { class: "info-label", "Path:" }
                    div { class: "path-container", 
                        code { class: "installation-path-code", "{installation_path_display}" }
                    }
                }
            }
        }
        
        // Usage statistics section
        div { class: "settings-section usage-stats",
            h3 { "Usage Statistics" }
            
            div { class: "stats-grid",
                div { class: "stat-item",
                    div { class: "stat-value", "{total_launches}" }
                    div { class: "stat-label", "Total Launches" }
                }
                
                div { class: "stat-item",
                    div { class: "stat-value",
                        {match last_launch {
                            Some(launch_date) => format!("{}", launch_date.format("%B %d, %Y")),
                            None => "Never".to_string(),
                        }}
                    }
                    div { class: "stat-label", "Last Launch" }
                }
            }
        }
        
        // Advanced section with backup integration
        div { class: "settings-section advanced-settings",
            h3 { "Advanced" }
            
            div { class: "advanced-description",
                p { "These settings allow you to directly manage your installation. Use with caution." }
            }
            
            // Backup & Restore option
            div { class: "advanced-option",
                div { class: "advanced-option-info",
                    h4 { "Backup & Restore" }
                    p { "Create backups of your installation and restore from previous states to protect your configuration and settings." }
                }
                
                button {
                    class: "advanced-button backup-button",
                    disabled: *is_operating.read(),
                    onclick: move |_| {
                        let current_state = *show_backup_section.read();
                        show_backup_section.set(!current_state);
                    },
                    {if *show_backup_section.read() {
                        "Hide Backup Options"
                    } else {
                        "Show Backup Options"
                    }}
                }
            }
            
            // Fixed: Use proper conditional rendering for backup section
            {if *show_backup_section.read() {
                rsx! {
                    div { class: "backup-expanded-section",
                        // Quick backup creation
                        div { class: "backup-quick-create",
                            h5 { "Quick Backup" }
                            
                            div { class: "backup-description-input",
                                input {
                                    r#type: "text",
                                    value: "{backup_description}",
                                    placeholder: "Backup description (optional)",
                                    oninput: move |evt| backup_description.set(evt.value().clone())
                                }
                            }
                            
                            div { class: "backup-quick-actions",
                                button {
                                    class: "backup-config-button",
                                    onclick: move |_| show_backup_config.set(true),
                                    "⚙️ Configure"
                                }
                                
                                button {
                                    class: "create-backup-button",
                                    disabled: *is_creating_backup.read(),
                                    onclick: create_backup,
                                    {if *is_creating_backup.read() {
                                        "Creating..."
                                    } else {
                                        "Create Backup"
                                    }}
                                }
                            }
                            
                            // Progress display
                            {backup_progress.read().as_ref().map(|progress| {
                                let progress_text = format!("Creating backup... {}/{} files", progress.files_processed, progress.total_files);
                                let progress_percentage = if progress.total_files > 0 { 
                                    (progress.files_processed as f64 / progress.total_files as f64 * 100.0) as u32 
                                } else { 
                                    0 
                                };
                                
                                rsx! {
                                    div { class: "backup-progress-mini",
                                        div { class: "progress-text", "{progress_text}" }
                                        div { class: "progress-bar-mini",
                                            div { 
                                                class: "progress-fill",
                                                style: "width: {progress_percentage}%"
                                            }
                                        }
                                    }
                                }
                            })}
                        }
                        
                        // Available backups list
                        div { class: "backup-list-section",
                            h5 { "Available Backups ({available_backups.read().len()})" }
                            
                            {if available_backups.read().is_empty() {
                                rsx! {
                                    div { class: "no-backups-mini",
                                        "No backups available. Create your first backup above."
                                    }
                                }
                            } else {
                                // Add state for showing all backups
                                let mut show_all_backups = use_signal(|| false);
                                let total_backups = available_backups.read().len();
                                let display_count = if *show_all_backups.read() { total_backups } else { 3 };
                                
                                rsx! {
                                    div { class: "backups-list-mini",
                                        {available_backups.read().iter().take(display_count).map(|backup| {
                                            // Clone all values we need before moving into closures
                                            let backup_id = backup.id.clone();
                                            let backup_id_for_onclick = backup_id.clone();
                                            let backup_id_for_delete = backup_id.clone();
                                            let installation_clone = installation.clone();
                                            let is_selected = selected_backup.read().as_ref() == Some(&backup_id);
                                            let age_desc = backup.age_description();
                                            let formatted_size = backup.formatted_size();
                                            let backup_desc = backup.description.clone();
                                            let backup_type_badge = match backup.backup_type {
                                                crate::backup::BackupType::Manual => "Manual",
                                                crate::backup::BackupType::PreUpdate => "Pre-Update", 
                                                crate::backup::BackupType::PreInstall => "Pre-Install",
                                                crate::backup::BackupType::Scheduled => "Scheduled",
                                            };
                                            
                                            rsx! {
                                                div { 
                                                    key: "{backup_id}",
                                                    class: if is_selected {
                                                        "backup-item-mini selected"
                                                    } else {
                                                        "backup-item-mini"
                                                    },
                                                    onclick: move |_| {
                                                        if is_selected {
                                                            selected_backup.set(None);
                                                        } else {
                                                            selected_backup.set(Some(backup_id_for_onclick.clone()));
                                                        }
                                                    },
                                                    
                                                    div { class: "backup-info-mini",
                                                        div { class: "backup-header-mini",
                                                            div { class: "backup-name", "{backup_desc}" }
                                                            span { 
                                                                class: "backup-type-badge",
                                                                style: match backup.backup_type {
                                                                    crate::backup::BackupType::Manual => "background: #28a745; color: white;",
                                                                    crate::backup::BackupType::PreUpdate => "background: #ffc107; color: black;",
                                                                    crate::backup::BackupType::PreInstall => "background: #17a2b8; color: white;",
                                                                    crate::backup::BackupType::Scheduled => "background: #6f42c1; color: white;",
                                                                },
                                                                "{backup_type_badge}"
                                                            }
                                                        }
                                                        div { class: "backup-meta", 
                                                            "{age_desc} • {formatted_size}"
                                                        }
                                                        
                                                        // Show more details when selected
                                                        {if is_selected {
                                                            rsx! {
                                                                div { class: "backup-details-mini",
                                                                    div { class: "backup-detail-row",
                                                                        span { class: "detail-label", "Version:" }
                                                                        span { class: "detail-value", "{backup.modpack_version}" }
                                                                    }
                                                                    div { class: "backup-detail-row",
                                                                        span { class: "detail-label", "Features:" }
                                                                        span { class: "detail-value", "{backup.enabled_features.len()}" }
                                                                    }
                                                                    div { class: "backup-detail-row",
                                                                        span { class: "detail-label", "Files:" }
                                                                        span { class: "detail-value", "{backup.file_count}" }
                                                                    }
                                                                }
                                                            }
                                                        } else {
                                                            rsx! { span {} }
                                                        }}
                                                    }
                                                    
                                                    {if is_selected {
                                                        rsx! {
                                                            div { class: "backup-actions-mini",
                                                                button {
                                                                    class: "restore-button-mini",
                                                                    onclick: move |evt| {
                                                                        evt.stop_propagation();
                                                                        show_restore_confirm.set(true);
                                                                    },
                                                                    "Restore"
                                                                }
                                                                
                                                                button {
                                                                    class: "delete-backup-button-mini",
                                                                    onclick: move |evt| {
                                                                        evt.stop_propagation();
                                                                        // Use pre-cloned values
                                                                        let backup_id_for_async = backup_id_for_delete.clone();
                                                                        let installation_for_async = installation_clone.clone();
                                                                        let mut available_backups_clone = available_backups;
                                                                        let mut operation_error_clone = operation_error;
                                                                        
                                                                        spawn(async move {
                                                                            match installation_for_async.delete_backup(&backup_id_for_async).await {
                                                                                Ok(_) => {
                                                                                    // Refresh backup list
                                                                                    if let Ok(backups) = installation_for_async.list_available_backups() {
                                                                                        available_backups_clone.set(backups);
                                                                                    }
                                                                                },
                                                                                Err(e) => {
                                                                                    operation_error_clone.set(Some(format!("Failed to delete backup: {}", e)));
                                                                                }
                                                                            }
                                                                        });
                                                                    },
                                                                    style: "background: #dc3545; color: white; margin-left: 8px;",
                                                                    "Delete"
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        rsx! { span {} }
                                                    }}
                                                }
                                            }
                                        })}
                                        
                                        // Show More/Less button
                                        {if total_backups > 3 {
                                            let remaining_count = total_backups - 3;
                                            rsx! {
                                                div { class: "backup-show-more",
                                                    button {
                                                        class: "backup-expand-button",
                                                        onclick: move |_| {
                                                            let current_state = *show_all_backups.read();
                                                            show_all_backups.set(!current_state);
                                                        },
                                                        
                                                        if *show_all_backups.read() {
                                                            "▲ Show Less"
                                                        } else {
                                                            "▼ Show {remaining_count} More Backups"
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            rsx! { span {} }
                                        }}
                                    }
                                    
                                    // Fixed: Add bulk actions section with proper conditional rendering
                                    {if available_backups.read().len() > 5 {
                                        // Clone installation before moving into closure
                                        let installation_for_bulk = installation.clone();
                                        
                                        rsx! {
                                            div { class: "backup-bulk-actions",
                                                details {
                                                    summary { "Bulk Actions" }
                                                    div { class: "bulk-actions-content",
                                                        button {
                                                            class: "bulk-action-button cleanup-old",
                                                            onclick: move |_| {
                                                                let installation_clone = installation_for_bulk.clone();
                                                                let mut available_backups_clone = available_backups;
                                                                let mut operation_error_clone = operation_error;
                                                                
                                                                spawn(async move {
                                                                    // Clean up backups older than 30 days, keeping at least 3
                                                                    let backups = match installation_clone.list_available_backups() {
                                                                        Ok(backups) => backups,
                                                                        Err(e) => {
                                                                            operation_error_clone.set(Some(format!("Failed to load backups: {}", e)));
                                                                            return;
                                                                        }
                                                                    };
                                                                    
                                                                    if backups.len() <= 3 {
                                                                        operation_error_clone.set(Some("Cannot cleanup - keeping minimum of 3 backups".to_string()));
                                                                        return;
                                                                    }
                                                                    
                                                                    let cutoff = chrono::Utc::now() - chrono::Duration::days(30);
                                                                    let mut deleted_count = 0;
                                                                    
                                                                    // Sort by date and keep newest 3, delete rest that are older than 30 days
                                                                    let mut sorted_backups = backups.clone();
                                                                    sorted_backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                                                                    
                                                                    // Delete backups one by one (since delete_backup is async)
                                                                    for backup in sorted_backups.iter().skip(3) {
                                                                        if backup.created_at < cutoff {
                                                                            match installation_clone.delete_backup(&backup.id).await {
                                                                                Ok(_) => deleted_count += 1,
                                                                                Err(e) => {
                                                                                    warn!("Failed to delete backup {}: {}", backup.id, e);
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                    
                                                                    // Refresh the list
                                                                    if let Ok(updated_backups) = installation_clone.list_available_backups() {
                                                                        available_backups_clone.set(updated_backups);
                                                                    }
                                                                    
                                                                    if deleted_count > 0 {
                                                                        operation_error_clone.set(Some(format!("Cleaned up {} old backups (30+ days old)", deleted_count)));
                                                                    } else {
                                                                        operation_error_clone.set(Some("No old backups found to clean up".to_string()));
                                                                    }
                                                                });
                                                            },
                                                            "🧹 Clean Up Old Backups (30+ days)"
                                                        }
                                                        
                                                        p { class: "bulk-action-info",
                                                            "Removes backups older than 30 days, keeping at least 3 most recent backups."
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        rsx! { span {} }
                                    }}
                                }
                            }}
                        }
                    }
                }
            } else {
                rsx! { span {} }
            }}
            
            div { class: "advanced-option",
                div { class: "advanced-option-info",
                    h4 { "Reset Installation Cache" }
                    p { 
                        "Clears all downloaded content (mods, shaders, configs) while preserving your installation settings and feature selections. " 
                        "This unlocks the install button for a fresh reinstall or backup restore. Use this if files are corrupted or you want to start fresh."
                    }
                    
                    // Add warning for clarity
                    div { class: "reset-cache-warning",
                        style: "margin-top: 8px; padding: 8px; background: rgba(255, 193, 7, 0.1); border-left: 3px solid #ffc107; font-size: 0.9rem;",
                        "⚠️ This will delete downloaded files but keep your feature choices and installation settings. You'll need to reinstall after resetting."
                    }
                }
                
                button {
                    class: "advanced-button reset-cache-button",
                    disabled: *is_operating.read(),
        
                    onclick: move |_| {
                        debug!("Reset cache clicked for installation: {}", installation_id_for_cache);
                        is_operating.set(true);
                        
                        let installation_path_for_cache = installation.installation_path.clone();
                        let mut operation_error_clone = operation_error;
                        let mut is_operating_clone = is_operating;
                        let installation_clone_for_async = installation.clone();
                        let onupdate_clone = onupdate;
                        let _installation_id_for_cache_async = installation_id_for_cache.clone();
                        
                        spawn(async move {
                            // Load the universal manifest to check can_reset flags
                            let http_client = crate::CachedHttpClient::new();
                            let universal_manifest = match crate::universal::load_universal_manifest(&http_client, None).await {
                                Ok(manifest) => Some(manifest),
                                Err(e) => {
                                    error!("Failed to load universal manifest for reset: {}", e);
                                    None
                                }
                            };
                            
                            // Standard cache folders (always reset)
                            let mut folders_to_reset = vec![
                                installation_path_for_cache.join("mods"),
                                installation_path_for_cache.join("resourcepacks"),
                                installation_path_for_cache.join("shaderpacks")
                            ];
                            
                            // Add resetable includes/remote includes if manifest is available
                            if let Some(manifest) = universal_manifest {
                                // Check includes
                                for include in &manifest.include {
                                    if include.can_reset {
                                        let include_path = installation_path_for_cache.join(&include.location);
                                        if include_path.exists() {
                                            folders_to_reset.push(include_path);
                                            debug!("Added include to reset list: {} (can_reset=true)", include.location);
                                        }
                                    }
                                }
                                
                                // Check remote includes
                                for remote in &manifest.remote_include {
                                    if remote.can_reset {
                                        let remote_path = if let Some(path) = &remote.path {
                                            installation_path_for_cache.join(path)
                                        } else {
                                            installation_path_for_cache.clone()
                                        };
                                        
                                        if remote_path.exists() && remote_path != installation_path_for_cache {
                                            folders_to_reset.push(remote_path);
                                            debug!("Added remote include to reset list: {} (can_reset=true)", 
                                                   remote.name.as_deref().unwrap_or(&remote.id));
                                        }
                                    }
                                }
                            }
                            
                            let mut success = true;
                            let mut folders_processed = 0;
                            
                            // Reset all identified folders
                            for folder in &folders_to_reset {
                                if folder.exists() {
                                    match std::fs::remove_dir_all(folder) {
                                        Ok(_) => {
                                            if let Err(e) = std::fs::create_dir_all(folder) {
                                                operation_error_clone.set(Some(format!("Failed to recreate folder {}: {}", 
                                                                                       folder.display(), e)));
                                                success = false;
                                                break;
                                            } else {
                                                debug!("Successfully reset folder: {}", folder.display());
                                                folders_processed += 1;
                                            }
                                        },
                                        Err(e) => {
                                            operation_error_clone.set(Some(format!("Failed to clear folder {}: {}", 
                                                                                   folder.display(), e)));
                                            success = false;
                                            break;
                                        }
                                    }
                                } else {
                                    // Create folder if it doesn't exist
                                    if let Err(e) = std::fs::create_dir_all(folder) {
                                        operation_error_clone.set(Some(format!("Failed to create folder {}: {}", 
                                                                               folder.display(), e)));
                                        success = false;
                                        break;
                                    } else {
                                        folders_processed += 1;
                                    }
                                }
                            }
                            
                            // CRITICAL FIX: Also remove the manifest.json to clear installation state
                            let manifest_path = installation_path_for_cache.join("manifest.json");
                            if manifest_path.exists() {
                                if let Err(e) = std::fs::remove_file(&manifest_path) {
                                    warn!("Failed to remove manifest.json during cache reset: {}", e);
                                    // Don't fail completely, but warn
                                } else {
                                    debug!("Removed manifest.json to reset installation state");
                                }
                            }
                            
                            is_operating_clone.set(false);
                            
                            if success {
                                // CRITICAL FIX: Mark installation as NOT installed to unlock install button
                                let mut installation_for_update = installation_clone_for_async.clone();
                                installation_for_update.installed = false;
                                installation_for_update.modified = false;
                                installation_for_update.update_available = false;
                                installation_for_update.preset_update_available = false;
                                
                                // Clear installed features but keep the user's selections
                                installation_for_update.installed_features.clear();
                                // Keep pending_features and enabled_features so user doesn't lose their choices
                                
                                if let Err(e) = installation_for_update.save() {
                                    operation_error_clone.set(Some(format!("Failed to update installation state: {}", e)));
                                } else {
                                    onupdate_clone.call(installation_for_update);
                                    
                                    // Success message
                                    operation_error_clone.set(Some(format!(
                                        "Cache successfully reset ({} folders cleared). Installation is now ready for fresh install/restore.", 
                                        folders_processed
                                    )));
                                }
                            }
                        });
                    },
                    "Reset Cache"
                }
            }
        }
        
        // Actions section
        div { class: "settings-section actions",
            h3 { "Actions" }
            
            div { class: "settings-actions",
                // Rename button
                button {
                    class: "settings-action-button rename-button",
                    disabled: *is_operating.read(),
                    onclick: move |_| {
                        new_name.set(installation_name.clone());
                        show_rename_dialog.set(true);
                    },
                    span { class: "action-icon", "✏️" }
                    "Rename Installation"
                }
                
                // Open folder button
                button {
                    class: "settings-action-button folder-button",
                    disabled: *is_operating.read(),
                    onclick: open_folder,
                    span { class: "action-icon", "📂" }
                    "Open Installation Folder"
                }
                
                // Delete button
                button {
                    class: "settings-action-button delete-button",
                    disabled: *is_operating.read(),
                    onclick: move |_| {
                        show_delete_confirm.set(true);
                    },
                    span { class: "action-icon", "🗑️" }
                    "Delete Installation"
                }
            }
        }
        
        // All the existing modals (rename, delete, backup config, restore confirm)
        // Rename dialog
        {if *show_rename_dialog.read() {
            Some(rsx! {
                div { class: "modal-overlay",
                    div { class: "modal-container rename-dialog",
                        div { class: "modal-header",
                            h3 { "Rename Installation" }
                            button { 
                                class: "modal-close",
                                disabled: *is_operating.read(),
                                onclick: move |_| {
                                    if !*is_operating.read() {
                                        show_rename_dialog.set(false);
                                    }
                                },
                                "×"
                            }
                        }
                        
                        div { class: "modal-content",
                            {rename_error.read().as_ref().map(|error| rsx! {
                                div { class: "rename-error", "{error}" }
                            })}
                            
                            div { class: "form-group",
                                label { r#for: "installation-name", "New name:" }
                                input {
                                    id: "installation-name",
                                    r#type: "text",
                                    value: "{new_name}",
                                    maxlength: "25",
                                    disabled: *is_operating.read(),
                                    oninput: move |evt| {
                                        let value = evt.value().clone();
                                        if value.len() <= 25 {
                                            new_name.set(value);
                                        }
                                    },
                                    placeholder: "Enter new installation name"
                                }
                                
                                div { class: "character-counter",
                                    style: if new_name.read().len() > 20 { 
                                        "color: #ff9d93;" 
                                    } else { 
                                        "color: rgba(255, 255, 255, 0.6);" 
                                    },
                                    "{new_name.read().len()}/25"
                                }
                            }
                        }
                        
                        div { class: "modal-footer",
                            button { 
                                class: "cancel-button",
                                disabled: *is_operating.read(),
                                onclick: move |_| {
                                    if !*is_operating.read() {
                                        show_rename_dialog.set(false);
                                    }
                                },
                                "Cancel"
                            }
                            
                            button { 
                                class: "save-button",
                                disabled: *is_operating.read(),
                                onclick: handle_rename,
                                {if *is_operating.read() {
                                    "Saving..."
                                } else {
                                    "Save"
                                }}
                            }
                        }
                    }
                }
            })
        } else {
            None
        }}
        
        // Delete confirmation dialog
        {if *show_delete_confirm.read() {
            Some(rsx! {
                div { class: "modal-overlay",
                    div { class: "modal-container delete-dialog",
                        div { class: "modal-header",
                            h3 { "Delete Installation" }
                            button { 
                                class: "modal-close",
                                disabled: *is_operating.read(),
                                onclick: move |_| {
                                    if !*is_operating.read() {
                                        show_delete_confirm.set(false);
                                    }
                                },
                                "×"
                            }
                        }
                        
                        div { class: "modal-content",
                            p { "Are you sure you want to delete this installation?" }
                            p { class: "delete-warning", "This action cannot be undone!" }
                            p { "Installation: ", strong { "{installation_name}" } }
                        }
                        
                        div { class: "modal-footer",
                            button { 
                                class: "cancel-button",
                                disabled: *is_operating.read(),
                                onclick: move |_| {
                                    if !*is_operating.read() {
                                        show_delete_confirm.set(false);
                                    }
                                },
                                "Cancel"
                            }
                            
                            button { 
                                class: "delete-button",
                                disabled: *is_operating.read(),
                                onclick: handle_delete,
                                {if *is_operating.read() {
                                    "Deleting..."
                                } else {
                                    "Delete"
                                }}
                            }
                        }
                    }
                }
            })
        } else {
            None
        }}
        
        // Backup configuration dialog
        {if *show_backup_config.read() {
            // Clone installation for the backup config dialog
            let installation_for_config = installation.clone();
            
            Some(rsx! {
                BackupConfigDialog {
                    config: backup_config,
                    estimated_size: installation_for_config.get_backup_size_estimate(&backup_config.read()).unwrap_or(0),
                    onclose: move |_| show_backup_config.set(false),
                    onupdate: move |new_config: BackupConfig| {
                        backup_config.set(new_config);
                    }
                }
            })
        } else {
            None
        }}
        
        // Restore confirmation dialog
        {if *show_restore_confirm.read() {
            Some(rsx! {
                RestoreConfirmDialog {
                    backup_id: selected_backup.read().clone().unwrap_or_default(),
                    backups: available_backups.read().clone(),
                    installation: installation.clone(),
                    onclose: move |_| show_restore_confirm.set(false),
                    onupdate: onupdate
                }
            })
        } else {
            None
        }}
    }
}
}

// Keep the existing BackupConfigDialog component
#[component]
fn BackupConfigDialog(
    config: Signal<crate::backup::BackupConfig>,
    estimated_size: u64,
    onclose: EventHandler<()>,
    onupdate: EventHandler<crate::backup::BackupConfig>,
) -> Element {
    let mut local_config = use_signal(|| config.read().clone());
    let backup_mode = use_signal(|| "custom".to_string()); // Start with custom mode
    
    // FIXED: Use the same important_folders array as SimplifiedBackupDialog
    let important_folders = ["wynntils".to_string(),
        "config".to_string(), 
        "mods".to_string(),
        ".bobby".to_string(),
        "Distant_Horizons_server_data".to_string()];
    
    // Initialize with pre-selected critical folders
    use_effect({
        let mut local_config = local_config;
        
        move || {
            let pre_selected = vec![
                "wynntils".to_string(),
                "config".to_string(),
                "mods".to_string(),
            ];
            local_config.with_mut(|c| {
                c.selected_items = pre_selected;
            });
        }
    });
    
    rsx! {
        div { class: "modal-overlay",
            div { class: "modal-container backup-config-dialog enhanced",
                div { class: "modal-header",
                    h3 { "Backup Configuration" }
                    button { 
                        class: "modal-close",
                        onclick: move |_| onclose.call(()),
                        "×"
                    }
                }
                
                div { class: "modal-content",
                    // FIXED: Add backup mode selection section
                    div { class: "backup-mode-section",
                        h4 { "Backup Type" }
                        
                        div { class: "backup-mode-options",
                            label { 
                                class: if backup_mode.read().as_str() == "complete" { 
                                    "backup-mode-option selected" 
                                } else { 
                                    "backup-mode-option" 
                                },
                                input {
                                    r#type: "radio",
                                    name: "backup-mode",
                                    value: "complete",
                                    checked: backup_mode.read().as_str() == "complete",
                                    onchange: {
                                        let mut local_config = local_config;
                                        let mut backup_mode = backup_mode;
                                        
                                        move |_| {
                                            backup_mode.set("complete".to_string());
                                            local_config.with_mut(|c| {
                                                c.selected_items = vec!["*".to_string()]; // Special marker for complete backup
                                            });
                                        }
                                    }
                                }
                                div { class: "mode-content",
                                    div { class: "mode-title", "📦 Complete Backup" }
                                    div { class: "mode-description", 
                                        "Backs up everything in your installation folder"
                                    }
                                }
                            }
                            
                            label { 
                                class: if backup_mode.read().as_str() == "custom" { 
                                    "backup-mode-option selected" 
                                } else { 
                                    "backup-mode-option" 
                                },
                                input {
                                    r#type: "radio",
                                    name: "backup-mode", 
                                    value: "custom",
                                    checked: backup_mode.read().as_str() == "custom",
                                    onchange: {
                                        let mut local_config = local_config;
                                        let mut backup_mode = backup_mode;
                                        
                                        move |_| {
                                            backup_mode.set("custom".to_string());
                                            let selected = vec![
                                                "wynntils".to_string(),
                                                "config".to_string(),
                                                "mods".to_string(),
                                            ];
                                            local_config.with_mut(|c| {
                                                c.selected_items = selected; // Pre-select most important ones
                                            });
                                        }
                                    }
                                }
                                div { class: "mode-content",
                                    div { class: "mode-title", "⚙️ Custom Backup" }
                                    div { class: "mode-description", 
                                        "Choose which specific folders to include"
                                    }
                                }
                            }
                        }
                    }
                    
                    // Show different content based on backup mode
                    {if backup_mode.read().as_str() == "complete" {
                        rsx! {
                            div { class: "complete-backup-preview",
                                h5 { "Complete backup will include:" }
                                div { class: "complete-backup-info",
                                    div { class: "backup-scope-description",
                                        "✅ All mod and configuration folders"
                                        br {}
                                        "✅ Resource packs, shader packs, and screenshots"  
                                        br {}
                                        "✅ World data (.bobby, Distant Horizons, saves)"
                                        br {}
                                        "✅ Any other custom folders you've added"
                                        br {}
                                        "❌ Excludes: logs, crash reports, and temporary files"
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {
                            div { class: "config-section",
                                h4 { "Select folders to backup:" }
                                
                                // FIXED: Use enhanced folder selection format
                                div { class: "folder-selection-list",
                                    for folder in important_folders.iter() {
                                        {
                                            let folder_name = folder.clone();
                                            let is_selected = local_config.read().selected_items.contains(&folder_name);
                                            
                                            rsx! {
                                                label { 
                                                    class: get_folder_selection_class(folder, is_selected),
                                                    input {
                                                        r#type: "checkbox",
                                                        checked: is_selected,
                                                        onchange: {
                                                            let folder_name = folder_name.clone();
                                                            let mut local_config = local_config;
                                                            
                                                            move |evt| {
                                                                let checked = evt.value() == "true";
                                                                local_config.with_mut(|c| {
                                                                    if checked {
                                                                        if !c.selected_items.contains(&folder_name) {
                                                                            c.selected_items.push(folder_name.clone());
                                                                        }
                                                                    } else {
                                                                        c.selected_items.retain(|p| p != &folder_name);
                                                                    }
                                                                });
                                                            }
                                                        }
                                                    }
                                                    
                                                    div { class: "folder-selection-content",
                                                        span { class: "folder-icon", "{get_folder_icon(folder)}" }
                                                        span { class: "folder-name", "{folder}" }
                                                        span { class: "folder-description", "{get_folder_description(folder)}" }
                                                        {if is_critical_folder(folder) {
                                                            rsx! { span { class: "folder-badge critical", "Critical" } }
                                                        } else if is_world_data_folder(folder) {
                                                            rsx! { span { class: "folder-badge world-data", "World Data" } }
                                                        } else {
                                                            rsx! { span {} }
                                                        }}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                div { class: "selection-summary",
                                    "Selected: {local_config.read().selected_items.len()} of {important_folders.len()} folders"
                                }
                            }
                        }
                    }}
                    
                    div { class: "config-section",
                        h4 { "Options:" }
                        
                        div { class: "options-list",
                            label { class: "option-item",
                                input {
                                    r#type: "checkbox",
                                    checked: local_config.read().compress_backups,
                                    onchange: move |evt| {
                                        local_config.with_mut(|c| c.compress_backups = evt.value() == "true");
                                    }
                                }
                                span { "Compress backup (recommended - saves ~35% space)" }
                            }
                            
                            label { class: "option-item",
                                input {
                                    r#type: "checkbox",
                                    checked: local_config.read().include_hidden_files,
                                    onchange: move |evt| {
                                        local_config.with_mut(|c| c.include_hidden_files = evt.value() == "true");
                                    }
                                }
                                span { "Include hidden files and folders (.bobby, .minecraft, etc.)" }
                            }
                            
                            div { class: "option-item number-option",
                                label { "Keep maximum:" }
                                input {
                                    r#type: "number",
                                    value: "{local_config.read().max_backups}",
                                    min: "1",
                                    max: "50",
                                    onchange: move |evt| {
                                        if let Ok(value) = evt.value().parse::<usize>() {
                                            local_config.with_mut(|c| c.max_backups = value);
                                        }
                                    }
                                }
                                span { "backups" }
                            }
                        }
                    }
                    
                    // Show estimated size
                    {if estimated_size > 0 {
                        rsx! {
                            div { class: "estimated-size",
                                "Estimated backup size: {crate::backup::format_bytes(estimated_size)}"
                                {if local_config.read().compress_backups {
                                    rsx! {
                                        span { class: "compression-note", 
                                            " (compressed: ~{crate::backup::format_bytes((estimated_size as f64 * 0.65) as u64)})"
                                        }
                                    }
                                } else {
                                    rsx! { span {} }
                                }}
                            }
                        }
                    } else {
                        rsx! { span {} }
                    }}
                }
                
                div { class: "modal-footer",
                    button { 
                        class: "cancel-button",
                        onclick: move |_| onclose.call(()),
                        "Cancel"
                    }
                    
                    button { 
                        class: "save-button",
                        disabled: local_config.read().selected_items.is_empty(),
                        onclick: move |_| {
                            onupdate.call(local_config.read().clone());
                            onclose.call(());
                        },
                        {
                            let count = local_config.read().selected_items.len();
                            if count == 0 {
                                "Select folders first".to_string()
                            } else if backup_mode.read().as_str() == "complete" {
                                "Save Complete Backup Configuration".to_string()
                            } else {
                                format!("Save Custom Backup Configuration ({} folders)", count)
                            }
                        }
                    }
                }
            }
        }
    }
}

// FIXED: Add the missing helper functions to settings_tab.rs
fn is_critical_folder(name: &str) -> bool {
    // Critical folders that are essential for mod functionality
    matches!(name, "wynntils" | "config" | "mods")
}

fn is_world_data_folder(name: &str) -> bool {
    // Folders that contain world-specific data
    matches!(name, ".bobby" | "Distant_Horizons_server_data")
}

fn get_folder_icon(name: &str) -> &'static str {
    match name {
        "wynntils" => "🎯",
        "config" => "⚙️", 
        "mods" => "🧩",
        ".bobby" => "🗺️",
        "Distant_Horizons_server_data" => "🌄",
        _ => "📁",
    }
}

fn get_folder_description(name: &str) -> &'static str {
    match name {
        "wynntils" => "Wynntils mod settings and data",
        "config" => "Mod configuration files",
        "mods" => "Installed mod files",
        ".bobby" => "Bobby world cache data",
        "Distant_Horizons_server_data" => "Distant Horizons world data",
        _ => "Custom folder",
    }
}

fn get_folder_selection_class(name: &str, is_selected: bool) -> String {
    let base = if is_critical_folder(name) {
        "folder-selection-item critical"
    } else if is_world_data_folder(name) {
        "folder-selection-item world-data"
    } else {
        "folder-selection-item"
    };
    
    if is_selected {
        format!("{} selected", base)
    } else {
        base.to_string()
    }
}

// New compact restore confirmation dialog
#[component]
fn RestoreConfirmDialog(
    backup_id: String,
    backups: Vec<BackupMetadata>,
    installation: Installation,
    onclose: EventHandler<()>,
    onupdate: EventHandler<Installation>,
) -> Element {
    let backup = backups.iter().find(|b| b.id == backup_id);
    let is_restoring = use_signal(|| false);
    let restore_error = use_signal(|| Option::<String>::None);
    
    let handle_restore = {
        let installation_clone = installation.clone();
        let backup_id_clone = backup_id.clone();
        let mut is_restoring = is_restoring;
        let mut restore_error = restore_error;
        let onupdate = onupdate;
        let onclose = onclose;
        
        move |_| {
            let mut installation = installation_clone.clone();
            let backup_id = backup_id_clone.clone();
            
            is_restoring.set(true);
            restore_error.set(None);
            
            spawn(async move {
                match installation.restore_from_backup(&backup_id).await {
                    Ok(_) => {
                        onupdate.call(installation);
                        onclose.call(());
                    },
                    Err(e) => {
                        restore_error.set(Some(format!("Failed to restore backup: {}", e)));
                        is_restoring.set(false);
                    }
                }
            });
        }
    };
    
    rsx! {
        div { class: "modal-overlay",
            div { class: "modal-container restore-confirm-dialog",
                div { class: "modal-header",
                    h3 { "Confirm Restore" }
                    button { 
                        class: "modal-close",
                        onclick: move |_| onclose.call(()),
                        "×"
                    }
                }
                
                div { class: "modal-content",
                    {if let Some(error) = &*restore_error.read() {
                        rsx! {
                            div { class: "error-notification",
                                div { class: "error-message", "{error}" }
                            }
                        }
                    } else {
                        rsx! { span {} }
                    }}
                    
                    div { class: "warning-message",
                        "⚠️ This will replace your current installation with the backup."
                    }
                    
                    {if let Some(backup) = backup {
                        rsx! {
                            div { class: "backup-details",
                                h4 { "Backup Details:" }
                                
                                div { class: "detail-grid",
                                    div { class: "detail-row",
                                        span { class: "detail-label", "Description:" }
                                        span { class: "detail-value", "{backup.description}" }
                                    }
                                    
                                    div { class: "detail-row",
                                        span { class: "detail-label", "Created:" }
                                        span { class: "detail-value", "{backup.age_description()}" }
                                    }
                                    
                                    div { class: "detail-row",
                                        span { class: "detail-label", "Version:" }
                                        span { class: "detail-value", "{backup.modpack_version}" }
                                    }
                                    
                                    div { class: "detail-row",
                                        span { class: "detail-label", "Features:" }
                                        span { class: "detail-value", "{backup.enabled_features.len()}" }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! { span {} }
                    }}
                    
                    div { class: "safety-info",
                        "💡 A safety backup will be created automatically before restoring."
                    }
                }
                
                div { class: "modal-footer",
                    button { 
                        class: "cancel-button",
                        disabled: *is_restoring.read(),
                        onclick: move |_| onclose.call(()),
                        "Cancel"
                    }
                    
                    button { 
                        class: "restore-confirm-button",
                        disabled: *is_restoring.read(),
                        onclick: handle_restore,
                        {if *is_restoring.read() {
                            "Restoring..."
                        } else {
                            "Restore Backup"
                        }}
                    }
                }
            }
        }
    }
}
