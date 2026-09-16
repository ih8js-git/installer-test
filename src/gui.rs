use dioxus::prelude::*;
use std::fs;
use std::panic;
use std::backtrace::Backtrace;
use std::env;
use platform_info::{PlatformInfo, PlatformInfoAPI, UNameAPI};
use simplelog::{CombinedLogger, TermLogger, WriteLogger, LevelFilter, TerminalMode, ColorChoice, Config as LogConfig};
use std::fs::File;
use dioxus::desktop::{Config as DioxusConfig, WindowBuilder, LogicalSize};
use dioxus::desktop::tao::window::Icon;
use std::collections::BTreeMap;
use std::path::PathBuf;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use modal::ModalContext;
use modal::Modal; 
use log::{debug, error, info, warn};
use isahc::ReadResponseExt;

use crate::{GithubBranch, build_http_client, GH_API, REPO, Config};
use crate::{get_app_data, get_launcher, uninstall, InstallerProfile, Launcher, PackName};
use crate::Installation;
use crate::installation;
use crate::CachedHttpClient;
use crate::changelog::Changelog as ChangelogData;
use crate::universal::ManifestError;
use crate::launcher::FeaturesTab;
use crate::launcher::PerformanceTab;

mod modal;


// Font constants
const HEADER_FONT: &str = "\"HEADER_FONT\"";
const REGULAR_FONT: &str = "\"REGULAR_FONT\"";
const ICON_BYTES: &[u8] = include_bytes!("assets/icon.png");

#[derive(Debug, Clone)]
struct TabInfo {
    color: String,
    title: String, 
    background: String,
    settings_background: String,
    modpacks: Vec<InstallerProfile>,
}

#[component]
fn PlayButton(
    uuid: String,
    disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {    
    rsx! {
        div { class: "play-button-container",
            button {
                class: "main-play-button",
                disabled: disabled,
                onclick: move |evt| onclick.call(evt),
                
                "PLAY"
            }
        }
    }
}

// Play button handler
pub fn handle_play_click(uuid: String, error_signal: &Signal<Option<String>>) {
    debug!("Play button clicked for modpack: {}", uuid);
    
    // Create a channel to communicate back to the main thread
    let (error_tx, error_rx) = std::sync::mpsc::channel::<String>();
    
    // Clone error_signal before moving to thread
    let mut error_signal_clone = *error_signal;
    
    // Launch the game using the proper launch function
    let uuid_clone = uuid.clone();
    std::thread::spawn(move || {
        match crate::launcher::launch_modpack(&uuid_clone) {
            Ok(_) => {
                debug!("Successfully launched modpack: {}", uuid_clone);
            },
            Err(e) => {
                error!("Failed to launch modpack: {}", e);
                let _ = error_tx.send(format!("Failed to launch modpack: {}", e));
            }
        }
    });
    
    // Create a task to check for errors from the background thread
    spawn(async move {
        if let Ok(error_message) = error_rx.recv() {
            error_signal_clone.set(Some(error_message));
        }
    });
}


#[component]
fn BackgroundParticles() -> Element {
    rsx! {
        div { class: "particles-container",
            // Basic floating particles - simplified to only circles
            for i in 0..30 {
                {
                    let size = 2 + (i % 6); // Size variation
                    let delay = (i as f32) * 0.4; // Staggered delays
                    let duration = 8.0 + (i % 10) as f32; // Duration variation
                    let left = 5 + (i * 3) % 90; // Better distribution
                    
                    // Simplified particle classes - only basic and glow
                    let particle_class = match i % 3 {
                        0 => "particle glow",
                        1 => "particle subtle",
                        _ => "particle",
                    };
                    
                    // Simpler animations - just float
                    let animation = match i % 2 {
                        0 => "float",
                        _ => "float-horizontal",
                    };
                    
                    rsx! {
                        div {
                            class: "{particle_class}",
                            style: "width: {size}px; height: {size}px; left: {left}%; 
                                bottom: -50px; opacity: {0.2 + (i % 3) as f32 * 0.15}; 
                                animation: {animation} {duration}s ease-in-out infinite {delay}s;"
                        }
                    }
                }
            }
            
            // Add some larger ambient orbs
            for i in 30..40 {
                {
                    let size = 12 + (i % 8);
                    let delay = (i as f32) * 1.5;
                    let duration = 20.0 + (i % 12) as f32;
                    let left = 10 + (i * 8) % 80;
                    let opacity = 0.1 + (i % 2) as f32 * 0.1;
                    
                    rsx! {
                        div {
                            class: "particle glow",
                            style: "width: {size}px; height: {size}px; left: {left}%; 
                                bottom: -80px; opacity: {opacity}; 
                                animation: float {duration}s ease-in-out infinite {delay}s;"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ErrorNotification(
    message: String,
    on_close: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "error-notification",
            div { class: "error-message", "{message}" }
            button { 
                class: "error-close",
                onclick: move |evt| on_close.call(evt),
                "×"
            }
        }
    }
}

#[component]
fn ManifestErrorDisplay(
    error: String,
    error_type: String,
    file_name: String,
    onclose: EventHandler<MouseEvent>,
    onreport: EventHandler<MouseEvent>,
) -> Element {
    let formatted_error = format_error_message(&error);
    
    rsx! {
        // Use the correct class names that match your CSS
        div { class: "error-syntax-overlay",
            div { class: "error-syntax-container",
                div { class: "error-syntax-header",
                    h2 { "{error_type} Error" }
                    button { 
                        class: "close-button",
                        onclick: move |evt| onclose.call(evt),
                        "×"
                    }
                }
                
                div { class: "error-syntax-content",
                    div { class: "error-syntax-message",
                        "There was a problem loading the {file_name} file. This could be due to:"
                    }
                    
                    ul { class: "error-reasons",
                        li { "A network connection issue" }
                        li { "A formatting problem in the file" }
                        li { "Missing or invalid data in the file" }
                    }
                    
                    div { class: "error-syntax-details",
                        "{formatted_error}"
                    }
                    
                    p { class: "error-help",
                        "Please copy these error details and report this issue so we can fix it."
                    }
                }
                
                div { class: "error-syntax-footer",
                    button { 
                        class: "cancel-button",
                        onclick: move |evt| onclose.call(evt),
                        "CLOSE"
                    }
                    
                    button { 
                        class: "retry-button",
                        onclick: move |evt| onreport.call(evt),
                        "REPORT ISSUE"
                    }
                    
                    button { 
                        class: "secondary-button",
                        onclick: move |_| {
                            // This would need to be implemented in your system
                            // Typically using web_sys::clipboard in WASM
                            debug!("Copying error to clipboard");
                        },
                        "COPY ERROR DETAILS"
                    }
                }
            }
        }
    }
}

// 2. Add a utility function to format error messages in a user-friendly way
fn format_error_message(error: &str) -> String {
    // Extract the most useful information from the error message
    
    if error.contains("failed to deserialize") || error.contains("missing field") {
        // Handle deserialization errors
        if let Some(field_name) = extract_field_name(error) {
            return format!("Problem with field: '{}'\n\nFull error: {}", field_name, error);
        }
    } else if error.contains("expected value") && error.contains("true") {
        return format!("Boolean value error: JSON requires lowercase 'true' and 'false'.\n\nFull error: {}", error);
    } else if error.contains("HTTP 404") {
        return format!("File not found (404). The file may have moved or been renamed.\n\nFull error: {}", error);
    } else if error.to_lowercase().contains("unexpected character") {
        return format!("JSON syntax error. There may be a missing comma, quote, or bracket.\n\nFull error: {}", error);
    }
    
    // Default case - return the original error
    error.to_string()
}

// Extract field name from deserialization errors
fn extract_field_name(error: &str) -> Option<String> {
    // Try to extract field name from common error patterns
    let patterns = [
        "missing field `", 
        "unknown field `",
        "invalid type for field `"
    ];
    
    for pattern in patterns {
        if let Some(start) = error.find(pattern) {
            let start_pos = start + pattern.len();
            if let Some(end) = error[start_pos..].find('`') {
                return Some(error[start_pos..(start_pos + end)].to_string());
            }
        }
    }
    
    None
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        match std::process::Command::new("cmd")
            .args(&["/c", "start", "", url])
            .spawn() {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("Failed to open URL: {}", e)),
            }
    }
    
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("open")
            .arg(url)
            .spawn() {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("Failed to open URL: {}", e)),
            }
    }
    
    #[cfg(target_os = "linux")]
    {
        match std::process::Command::new("xdg-open")
            .arg(url)
            .spawn() {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("Failed to open URL: {}", e)),
            }
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err("Opening URLs is not supported on this platform".to_string())
    }
}

// Updated main function to initialize the app correctly
fn main() {
    // Initialize logger
    fs::create_dir_all(get_app_data().join(".WC_OVHL/")).expect("Failed to create config dir!");
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Debug,
            simplelog::ConfigBuilder::new().add_filter_ignore_str("isahc::handler").build(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            LogConfig::default(),
            File::create(get_app_data().join(".WC_OVHL/installer.log")).unwrap(),
        ),
    ])
    .unwrap();

    // Set panic hook for error reporting
    panic::set_hook(Box::new(|info| {
        let payload = if let Some(string) = info.payload().downcast_ref::<String>() {
            string.to_string()
        } else if let Some(str) = info.payload().downcast_ref::<&'static str>() {
            str.to_string()
        } else {
            format!("{:?}", info.payload())
        };
        let backtrace = Backtrace::force_capture();
        error!("The installer panicked! This is a bug.\n{info:#?}\nPayload: {payload}\nBacktrace: {backtrace}");
    }));

    // Log initialization info
    info!("Installer version: {}", env!("CARGO_PKG_VERSION"));
    let platform_info = PlatformInfo::new().expect("Unable to determine platform info");
    debug!("System information:\n\tSysname: {}\n\tRelease: {}\n\tVersion: {}\n\tArchitecture: {}\n\tOsname: {}",
        platform_info.sysname().to_string_lossy(), 
        platform_info.release().to_string_lossy(), 
        platform_info.version().to_string_lossy(), 
        platform_info.machine().to_string_lossy(), 
        platform_info.osname().to_string_lossy()
    );

    // Workaround for NVIDIA driver issues on Linux
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/dev/dri").exists() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            warn!("Disabled hardware acceleration as a workaround for NVIDIA driver issues")
        }
    }

    // Load icon
    let icon = image::load_from_memory(include_bytes!("assets/icon.png")).unwrap();
    
    // Load branches
    let branches: Vec<GithubBranch> = serde_json::from_str(
        build_http_client()
            .get(GH_API.to_owned() + REPO + "branches")
            .expect("Failed to retrieve branches!")
            .text()
            .unwrap()
            .as_str(),
    )
    .expect("Failed to parse branches!");

    // Load configuration
    let config_path = get_app_data().join(".WC_OVHL/config.json");
    let config: Config;

    // Load or create config
    if config_path.exists() {
        config = serde_json::from_slice(&fs::read(&config_path).expect("Failed to read config!"))
            .expect("Failed to load config!");
    } else {
        config = Config {
            launcher: String::from("vanilla"),
            first_launch: Some(true),
        };
        fs::write(&config_path, serde_json::to_vec(&config).unwrap())
            .expect("Failed to write config!");
    }
    
    info!("Running installer with config: {config:#?}");
    
    // Load all installations (or empty vector if error)
    let installations = installation::load_all_installations().unwrap_or_default();
    
    // Launch the UI
    LaunchBuilder::desktop().with_cfg(
        DioxusConfig::new().with_window(
            WindowBuilder::new()
                .with_resizable(true)
                .with_title("Majestic Overhaul Launcher")
                .with_inner_size(LogicalSize::new(1280, 720))
                .with_min_inner_size(LogicalSize::new(960, 540))
        ).with_icon(
            Icon::from_rgba(icon.to_rgba8().to_vec(), icon.width(), icon.height()).unwrap(),
        ).with_data_directory(
            env::temp_dir().join(".WC_OVHL")
        ).with_menu(None)
    ).with_context(AppProps {
        branches,
        modpack_source: String::from(REPO),
        config,
        config_path,
        installations,
    }).launch(app);
}

#[component]
fn ChangelogSection(changelog: Option<ChangelogData>) -> Element {
    let mut show_all = use_signal(|| false);
    
    match changelog {
        Some(changelog_data) if !changelog_data.entries.is_empty() => {
            let display_count = if *show_all.read() { changelog_data.entries.len() } else { 3 };
            
            rsx! {
                div { class: "changelog-container",
                    div { class: "section-divider with-title", 
                        span { class: "divider-title", "LATEST CHANGES" }
                    }
                    
                    div { class: "changelog-entries",
                        for (index, entry) in changelog_data.entries.iter().enumerate().take(display_count) {
                            div { 
                                class: "changelog-entry",
                                "data-importance": "{entry.importance.clone().unwrap_or_else(|| String::from(\"normal\"))}",
                                
                                div { class: "changelog-header",
                                    h3 { class: "changelog-title", "{entry.title}" }
                                    
                                    if let Some(version) = &entry.version {
                                        span { class: "changelog-version", "v{version}" }
                                    }
                                    
                                    if let Some(date) = &entry.date {
                                        span { class: "changelog-date", "{date}" }
                                    }
                                }
                                
                                div { 
                                    class: "changelog-content",
                                    dangerous_inner_html: "{entry.contents}"
                                }
                                
                                if index < display_count - 1 {
                                    div { class: "entry-divider" }
                                }
                            }
                        }
                        
                        if changelog_data.entries.len() > 3 {
                            div { class: "view-all-changes",
                                button { 
                                    class: "view-all-button",
onclick: move |_| {
    let current_state = *show_all.read();
    show_all.set(!current_state);
},
                                    if *show_all.read() {
                                        "Show Less"
                                    } else {
                                        {format!("View {} More Changes", changelog_data.entries.len() - 3)}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        _ => {
            rsx! { Fragment {} }
        }
    }
}

// Add this new component for the footer with Discord button
#[component]
fn Footer(changelog: Option<ChangelogData>) -> Element {
    // Get footer button config from changelog or use defaults
    let footer_button = if let Some(changelog_data) = &changelog {
        if let Some(config) = &changelog_data.homepage_config {
            &config.footer_button
        } else {
            &crate::changelog::FooterButton::default()
        }
    } else {
        &crate::changelog::FooterButton::default()
    };

    rsx! {
        footer { class: "modern-footer",
            div { class: "footer-info",
                div { class: "footer-section",
                    p { class: "footer-text", 
                        "Modpacks and installer made by the Overhaul Team"
                    }
                }
                
                div { class: "footer-divider" }
                
                div { class: "footer-section",
                    p { class: "copyright", "© 2023-2025 Majestic Overhaul. CC BY-NC-SA 4.0." }
                }
            }
            
            // Dynamic footer button
            a {
                class: "footer-action-button install",
                href: "{footer_button.link}",
                target: "_blank",
                rel: "noopener noreferrer",
                style: "text-decoration: none;",
                
                "{footer_button.text}"
            }
        }
    }
}

// Home Page component with redundancy removed
#[component]
fn HomePage(
    installations: Signal<Vec<Installation>>,
    error_signal: Signal<Option<String>>,
    changelog: Signal<Option<ChangelogData>>,
    current_installation_id: Signal<Option<String>>,
) -> Element {
    // State for the installation creation dialog
    let mut show_creation_dialog = use_signal(|| false);
    
    // Check if this is the first time (no installations)
    let has_installations = !installations().is_empty();
    let _latest_installation = installations().first().cloned();
    
    rsx! {
        div { class: "home-container home-page",
            // Error notification if any
            if let Some(error) = error_signal() {
                ErrorNotification {
                    message: error,
                    on_close: move |_| {
                        error_signal.set(None);
                    }
                }
            }
            
            if has_installations {
                // Regular home page with installations
                
                // Statistics display - pass changelog data
                StatisticsDisplay { changelog: changelog() }
                
                // Section divider for installations
                div { class: "section-divider with-title", 
                    span { class: "divider-title", "YOUR INSTALLATIONS" }
                }
                
                // Grid of installation cards
                div { class: "installations-grid",
                    // Existing installation cards
                    for installation in installations() {
                        {
                            let installation_id = installation.id.clone();
                            rsx! {
                                InstallationCard { 
                                    installation: installation.clone(),
                                    onclick: move |_| {
                                        debug!("Clicked installation: {}", installation_id);
                                        current_installation_id.set(Some(installation_id.clone()));
                                    }
                                }
                            }
                        }
                    }
                    
                    // Create new installation card
                    div { 
                        class: "installation-card new-installation",
                        onclick: move |_| show_creation_dialog.set(true),
                        
                        div { class: "installation-card-content", 
                            div { class: "installation-card-icon", "+" }
                            h3 { "Create New Installation" }
                            p { "Set up a new Wynncraft experience" }
                        }
                    }
                }
            } else {
                // First-time user experience
                div { class: "welcome-container first-time",
                    h1 { "Welcome to the MAJESTIC OVERHAUL" }
                    p { "Optimized performance and improved visuals!" }
                    
                    // Statistics for first-time users too - pass changelog data
                    StatisticsDisplay { changelog: changelog() }
                    
                    button {
                        class: "main-install-button",
                        onclick: move |_| {
                            show_creation_dialog.set(true);
                        },
                        "Get Started"
                    }
                }
            }
            
            // Recent changes section
            ChangelogSection { changelog: changelog() }
            
            // Footer with dynamic data - pass changelog data
            Footer { changelog: changelog() }
            
            // Installation creation dialog
            if *show_creation_dialog.read() {
                SimplifiedInstallationWizard {
                    onclose: move |_| {
                        show_creation_dialog.set(false);
                    },
                    oncreate: move |new_installation: Installation| {
                        // Add the new installation to the list
                        installations.with_mut(|list| {
                            list.insert(0, new_installation.clone());
                        });
                        
                        // Close the dialog
                        show_creation_dialog.set(false);
                        
                        // Set the current installation to navigate to the installation page
                        current_installation_id.set(Some(new_installation.id));
                    }
                }
            }
        }
    }
}

// Special value for home page
const HOME_PAGE: usize = usize::MAX;

#[component]
fn InstallationCard(
    installation: Installation,
    onclick: EventHandler<String>,
) -> Element {
    // Format last played date
    let last_played = installation.last_launch.map(|dt| {
        dt.format("%B %d, %Y").to_string()
    });
    
    // Clone the ID for event handlers
    let installation_id = installation.id.clone();
    let play_id = installation.id.clone();
    let mut error_signal = use_signal(|| Option::<String>::None);
    
    rsx! {
        div { 
            class: "installation-card",
            "data-id": "{installation.id}",
            
            div { class: "installation-card-header",
                h3 { "{installation.name}" }
                
                if installation.update_available {
                    span { 
                        class: "update-badge", 
                        if installation.preset_update_available && !installation.update_available {
                            "Preset Update"
                        } else if !installation.preset_update_available && installation.update_available {
                            "Modpack Update"  
                        } else {
                            "Updates Available"
                        }
                    }
                }
            }
            
            div { class: "installation-card-details",
                div { class: "detail-item",
                    span { class: "detail-label", "Minecraft:" }
                    span { class: "detail-value", "{installation.minecraft_version}" }
                }
                
                div { class: "detail-item",
                    span { class: "detail-label", "Loader:" }
                    span { class: "detail-value", "{installation.loader_type} {installation.loader_version}" }
                }
                                
                div { class: "detail-item",
                    span { class: "detail-label", "Last Played:" }
                    span { class: "detail-value",
                        if let Some(last) = last_played {
                            {last}
                        } else {
                            {"Never"}
                        }
                    }
                }
                
                div { class: "detail-item memory-detail",
                    span { class: "detail-label", "Memory:" }
                    span { class: "detail-value", "{installation.memory_allocation} MB" }
                }
            }
            
            div { class: "installation-card-actions",
                button { 
                    class: "play-button",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        handle_play_click(play_id.clone(), &error_signal);
                    },
                    "Play"
                }
                
                button { 
                    class: "manage-button",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        onclick.call(installation_id.clone());
                    },
                    "Manage"
                }
            }
            
            // Display error if any occurs during play
            if let Some(error) = error_signal() {
                ErrorNotification {
                    message: error,
                    on_close: move |_| {
                        error_signal.set(None);
                    }
                }
            }
        }
    }
}

// Statistics display component
#[component]
fn StatisticsDisplay(changelog: Option<ChangelogData>) -> Element {
    // Get stats from changelog or use defaults
    let stats = if let Some(changelog_data) = &changelog {
        if let Some(config) = &changelog_data.homepage_config {
            &config.stats
        } else {
            // Use default stats if no config present
            &crate::changelog::HomePageStats::default()
        }
    } else {
        &crate::changelog::HomePageStats::default()
    };

    rsx! {
        div { class: "stats-container",
            div { class: "stat-item",
                span { class: "stat-value", "{stats.stat1_value}" }
                span { class: "stat-label", "{stats.stat1_label}" }
            }
            div { class: "stat-item",
                span { class: "stat-value", "{stats.stat2_value}" }
                span { class: "stat-label", "{stats.stat2_label}" } 
            }
            div { class: "stat-item",
                span { class: "stat-value", "{stats.stat3_value}" }
                span { class: "stat-label", "{stats.stat3_label}" }
            }
        }
    }
}

// Fixed: Combined and unified this struct to remove duplication
#[derive(PartialEq, Props, Clone)]
pub struct InstallationCreationProps {
    pub onclose: EventHandler<()>,
    pub oncreate: EventHandler<Installation>,
}

#[component]
pub fn SimplifiedInstallationWizard(props: InstallationCreationProps) -> Element {
    // State for installation name only
    let mut name = use_signal(|| "My Wynncraft Installation".to_string());
    let mut installation_error = use_signal(|| Option::<String>::None);
    let mut manifest_error = use_signal(|| Option::<ManifestError>::None);
    
    // Character limit for installation names
    const MAX_NAME_LENGTH: usize = 15;
    
    // Suggested names based on existing installations count
    let app_props = use_context::<AppProps>();
    let installations = app_props.installations;
    let selected_launcher = app_props.config.launcher.clone();
    let installation_count = installations.len() + 1;
    let suggested_names = vec![
        format!("Overhaul {}", installation_count),
        format!("Wynncraft {}", installation_count),
        format!("Majestic {}", installation_count),
        "My Overhaul".to_string(),
        "Custom Build".to_string(),
        "Wynn Go Brrr".to_string(),
    ];
    
    // Set default name based on count
    use_effect(move || {
        if *name.read() == "My Wynncraft Installation" {
            name.set(format!("Overhaul {}", installation_count));
        }
    });
    
    // Resource for universal manifest with better error handling
    let manifest_error_clone = manifest_error;
    let universal_manifest = use_resource(move || {
        let mut manifest_error = manifest_error_clone;
        async move {
            debug!("Loading universal manifest...");
            match crate::universal::load_universal_manifest(
                &crate::CachedHttpClient::new(), 
                Some("https://cdn.jsdelivr.net/gh/Wynncraft-Overhaul/majestic-overhaul@latest/universal.json")
            ).await {
                Ok(manifest) => {
                    debug!("Successfully loaded universal manifest: {}", manifest.name);
                    Some(manifest)
                },
                Err(e) => {
                    error!("Failed to load universal manifest: {}", e);
                    spawn(async move {
                        manifest_error.set(Some(e.clone()));
                    });
                    None
                }
            }
        }
    });
    
    // Function to create the installation
    let create_installation = move |_| {
        debug!("Creating installation with name: {}", name.read());

        // Validate name length
        let installation_name = name.read().trim().to_string();
        if installation_name.is_empty() {
            installation_error.set(Some("Installation name cannot be empty.".to_string()));
            return;
        }
        
        if installation_name.len() > MAX_NAME_LENGTH {
            installation_error.set(Some(format!("Installation name cannot exceed {} characters.", MAX_NAME_LENGTH)));
            return;
        }
        
        // Get the universal manifest for Minecraft version and loader information
        if let Some(unwrapped_manifest) = universal_manifest.read().as_ref().and_then(|opt| opt.as_ref()) {
            let minecraft_version = unwrapped_manifest.minecraft_version.clone();
            let loader_type = unwrapped_manifest.loader.r#type.clone();
            let loader_version = unwrapped_manifest.loader.version.clone();

            // Create a basic custom installation with proper defaults
            let mut installation = Installation::new_custom(
                installation_name.clone(),
                minecraft_version,
                loader_type,
                loader_version,
                selected_launcher.clone(),
                unwrapped_manifest.modpack_version.clone(),
            );

            // CRITICAL FIX: Initialize with default-enabled features from universal manifest
            let _http_client = crate::CachedHttpClient::new();
            let unwrapped_manifest_clone = unwrapped_manifest.clone();
            
            spawn(async move {
                // Build list of default features
                let mut default_features = vec!["default".to_string()];
                
                // Add all default-enabled components
                for component in &unwrapped_manifest_clone.mods {
                    if component.default_enabled && component.id != "default" && !default_features.contains(&component.id) {
                        default_features.push(component.id.clone());
                        debug!("Added default mod to new installation: {}", component.id);
                    }
                }
                
                for component in &unwrapped_manifest_clone.shaderpacks {
                    if component.default_enabled && component.id != "default" && !default_features.contains(&component.id) {
                        default_features.push(component.id.clone());
                        debug!("Added default shaderpack to new installation: {}", component.id);
                    }
                }
                
                for component in &unwrapped_manifest_clone.resourcepacks {
                    if component.default_enabled && component.id != "default" && !default_features.contains(&component.id) {
                        default_features.push(component.id.clone());
                        debug!("Added default resourcepack to new installation: {}", component.id);
                    }
                }
                
                for include in &unwrapped_manifest_clone.include {
                    if include.default_enabled && !include.id.is_empty() && include.id != "default" 
                       && !default_features.contains(&include.id) {
                        default_features.push(include.id.clone());
                        debug!("Added default include to new installation: {}", include.id);
                    }
                }
                
                for remote in &unwrapped_manifest_clone.remote_include {
                    if remote.default_enabled && remote.id != "default" 
                       && !default_features.contains(&remote.id) {
                        default_features.push(remote.id.clone());
                        debug!("Added default remote include to new installation: {}", remote.id);
                    }
                }
                
                // Initialize the installation with default features
                installation.enabled_features = default_features.clone();
                installation.pending_features = default_features.clone();
                installation.pre_install_features = default_features.clone();
                installation.is_custom_configuration = true;
                installation.selected_preset_id = None;
                
                debug!("Created custom installation with {} default features: {:?}", 
                       installation.enabled_features.len(), installation.enabled_features);
                
                // Mark as fresh installation
                installation.mark_as_fresh();
                
                // Register the installation
                if let Err(e) = crate::installation::register_installation(&installation) {
                    error!("Failed to register installation: {}", e);
                    installation_error.set(Some(format!("Failed to register installation: {}", e)));
                    return;
                }
                
                // Save the installation
                if let Err(e) = installation.save() {
                    error!("Failed to save installation: {}", e);
                    installation_error.set(Some(format!("Failed to save installation: {}", e)));
                    return;
                }
                
                debug!("Successfully created installation: {}", installation.id);
                
                // Call the oncreate handler to finalize
                props.oncreate.call(installation);
            });
        } else {
            error!("Universal manifest not available");
            installation_error.set(Some("Failed to load modpack information. Please try again.".to_string()));
        }
    };
    
    rsx! {
        div { class: "wizard-overlay",
            div { class: "installation-wizard",
                // Header with close button in corner
                div { class: "wizard-header",
                    h2 { "Create New Installation" }
                    button { 
                        class: "close-button",
                        onclick: move |_| props.onclose.call(()),
                        "×"
                    }
                }
                
                // Main content
                div { class: "wizard-content",
                    // Error notification if any
                    if let Some(error) = &*installation_error.read() {
                        div { class: "error-notification",
                            div { class: "error-message", "{error}" }
                            button {
                                class: "error-close",
                                onclick: move |_| installation_error.set(None),
                                "×"
                            }
                        }
                    }
                    
                    // Name section
                    div { class: "wizard-section",
                        h3 { "Installation Name" }
                        div { class: "form-group",
                            label { r#for: "installation-name", "Name your installation:" }
                            input {
                                id: "installation-name",
                                r#type: "text",
                                value: "{name}",
                                maxlength: "{MAX_NAME_LENGTH}",
                                oninput: move |evt| {
                                    let new_value = evt.value().clone();
                                    if new_value.len() <= MAX_NAME_LENGTH {
                                        name.set(new_value);
                                    }
                                },
                                placeholder: "e.g. My Installation"
                            }
                            
                            // Character counter
                            div { class: "character-counter",
                                style: if name.read().len() > MAX_NAME_LENGTH - 5 { 
                                    "color: #ff9d93;" 
                                } else { 
                                    "color: rgba(255, 255, 255, 0.6);" 
                                },
                                "{name.read().len()}/{MAX_NAME_LENGTH}"
                            }
                        }
                        
                        // Suggested names
                        div { class: "suggested-names",
                            span { class: "suggestion-label", "Quick suggestions:" }
                            div { class: "suggestion-chips",
                                for suggestion in suggested_names {
                                    button {
                                        class: "suggestion-chip",
                                        r#type: "button",
                                        onclick: move |_| name.set(suggestion.clone()),
                                        "{suggestion}"
                                    }
                                }
                            }
                        }
                    }
                    
                    // Minecraft info section - just to show what they're creating
                    if let Some(unwrapped_manifest) = universal_manifest.read().as_ref().and_then(|opt| opt.as_ref()) {
                        div { class: "wizard-section minecraft-info",
                            div { class: "info-row",
                                div { class: "info-item",
                                    span { class: "info-label", "Minecraft:" }
                                    span { class: "info-value", "{unwrapped_manifest.minecraft_version}" }
                                }
                                
                                div { class: "info-item",
                                    span { class: "info-label", "Loader:" }
                                    span { class: "info-value", "{unwrapped_manifest.loader.r#type} {unwrapped_manifest.loader.version}" }
                                }
                            }
                            
                            // Show what will be included by default
                            div { class: "default-features-info",
                                p { class: "info-description", 
                                    "You can customize further in the next step."
                                }
                            }
                        }
                    } else {
                        div { class: "loading-section",
                            div { class: "loading-spinner" }
                            div { class: "loading-text", "Loading modpack information..." }
                        }
                    }
                }
                
                // Footer with buttons
                div { class: "wizard-footer",
                    button {
                        class: "cancel-button",
                        onclick: move |_| props.onclose.call(()),
                        "Cancel"
                    }
                    
                    button {
                        class: "create-button",
                        disabled: universal_manifest.read().is_none(),
                        onclick: create_installation,
                        "Create Installation"
                    }
                }
            }
            
            // Add manifest error display
            if let Some(error) = manifest_error() {
                ManifestErrorDisplay {
                    error: error.message.clone(),
                    error_type: format!("{}", error.error_type),
                    file_name: error.file_name.clone(),
                    onclose: move |_| manifest_error.set(None),
                    onreport: move |_| {
                        let _ = open_url("https://discord.com/channels/778965021656743966/1234506784626970684");
                    }
                }
            }
        }
    }
}

// Installation management page
#[component]
pub fn InstallationManagementPage(
    installation_id: String,
    onback: EventHandler<()>,
    installations: Signal<Vec<Installation>>,
) -> Element {
    // Get icon base64 for header
    let icon_base64 = {
        use base64::{Engine, engine::general_purpose::STANDARD};
        Some(STANDARD.encode(include_bytes!("assets/icon.png")))
    };
    
    // State for the current tab
    let mut active_tab = use_signal(|| "features");

    // Clone installation_id BEFORE moving it into use_memo
    let _installation_id_for_delete = installation_id.clone();
    let _installation_id_for_launch = installation_id.clone();
    let _installation_id_for_update = installation_id.clone();
    let _installation_id_for_clear = installation_id.clone();
    let installation_id_for_memo = installation_id.clone();

    // Load the installation data
    let installation_result = use_memo(move || {
        crate::installation::load_installation(&installation_id_for_memo)
    });

    // Installation status signals
    let mut is_installing = use_signal(|| false);
    let mut installation_error = use_signal(|| Option::<String>::None);
    
    // Progress tracking signals
    let installation_progress = use_signal(|| 0i64);
    let installation_total = use_signal(|| 0i64);
    let installation_status = use_signal(String::new);
    
    // Handle installation not found
    if let Err(e) = &*installation_result.read() {
        return rsx! {
            div { class: "error-container",
                h2 { "Installation Not Found" }
                p { "The requested installation could not be found." }
                p { "Error: {e}" }
                
                button {
                    class: "back-button",
                    onclick: move |_| onback.call(()),
                    "Back to Home"
                }
            }
        };
    }
    
    // Unwrap installation from result
    let installation = installation_result.read().as_ref().unwrap().clone();
    
    // Create the installation state signal
    let installation_state = use_signal(|| installation.clone());
    
    // Clone needed values
    let installation_id_for_delete = installation.id.clone();
    let installation_id_for_launch = installation.id.clone();
    let installation_for_update = installation.clone();
    let _installation_for_preset_update = installation.clone();
    let _installation_for_features = installation.clone();
    
    // State for modification tracking
    let mut has_changes = use_signal(|| false);
    let enabled_features = use_signal(|| installation.enabled_features.clone());
    let memory_allocation = use_signal(|| installation.memory_allocation);
    let java_args = use_signal(|| installation.java_args.clone());
    let selected_preset = use_signal(|| Option::<String>::None);
    
    // State for tracking modifications in different areas 
    let features_modified = use_signal(|| false);
    let performance_modified = use_signal(|| false);
    
    // Filter text for feature search
    let filter_text = use_signal(String::new);

    // Add this with other state declarations
    let mut show_update_warning = use_signal(|| false);
    
    // Preset update message signal
    let _preset_update_msg = use_signal(|| Option::<String>::None);

    // Effect to detect changes
    use_effect({
        let enabled_features_for_effect = enabled_features;
        let original_features = installation.enabled_features.clone();
        let mut features_modified_copy = features_modified;
        
        move || {
            let features_changed = enabled_features_for_effect.read().clone() != original_features;
            
            // Update specific modification flags
            features_modified_copy.set(features_changed);
            
            // Only set has_changes for feature changes, not memory changes
            has_changes.set(features_changed);
        }
    });
    
    // Handle install/update with progress tracking
    let installation_for_update_clone = installation_for_update.clone();

    // Define the actual update process
// Updated proceed_with_update function with proper progress tracking


let mut proceed_with_update = {
    let installation_for_update_clone = installation_for_update_clone.clone();
    let enabled_features = enabled_features;
    let memory_allocation = memory_allocation;
    let java_args = java_args;
    let mut is_installing = is_installing;
    let installation_error = installation_error;
    let mut installation_progress = installation_progress;
    let installation_total = installation_total;
    let mut installation_status = installation_status;
    let has_changes = has_changes;
    let features_modified = features_modified;
    let performance_modified = performance_modified;
    let installations = installations;
    let installation_state = installation_state;
    let selected_preset = selected_preset;
    let installation_id_for_clear = installation_id.clone(); // Add this for session clearing
    
    move || {
        is_installing.set(true);
        
        // Reset progress before starting
        installation_progress.set(0);
        installation_status.set("Preparing installation...".to_string());
        
        let mut installation_clone = installation_for_update_clone.clone();


        // Save the user's current selections as pending
        let current_features = enabled_features.read().clone();
        let current_preset = selected_preset.read().clone();
        
        debug!("Saving pre-install selections - preset: {:?}, features: {:?}", current_preset, current_features);
        installation_clone.save_pre_install_selections(current_preset, current_features.clone());
        
        // Update settings
        installation_clone.enabled_features = current_features;
        installation_clone.memory_allocation = *memory_allocation.read();
        installation_clone.java_args = java_args.read().clone();
        installation_clone.modified = true;
        
        let http_client = crate::CachedHttpClient::new();
        let mut installation_error_clone = installation_error;
        let mut progress = installation_progress;
        let mut total = installation_total;
        let mut status = installation_status;
        let mut is_installing_clone = is_installing;
        let mut has_changes_clone = has_changes;
        let mut features_modified_clone = features_modified;
        let mut performance_modified_clone = performance_modified;
        let mut installations = installations;
        let mut installation_state = installation_state;
        let installation_id = installation_clone.id.clone();
        let installation_id_for_clear_async = installation_id_for_clear.clone(); // Clone for async

        spawn(async move {
            // Calculate total items for accurate progress tracking
            match crate::universal::load_universal_manifest(&http_client, None).await {
                Ok(manifest) => {
                    let enabled_features = installation_clone.enabled_features.clone();
                    
                    // Count all components that will be processed
                    let mut total_items = 0;
                    
                    // Count mods (including default ones)
                    total_items += manifest.mods.iter()
                        .filter(|m| {
                            if m.id == "default" || !m.optional {
                                true
                            } else {
                                enabled_features.contains(&m.id)
                            }
                        })
                        .count();
                    
                    // Count shaderpacks
                    total_items += manifest.shaderpacks.iter()
                        .filter(|s| {
                            if s.id == "default" || !s.optional {
                                true
                            } else {
                                enabled_features.contains(&s.id)
                            }
                        })
                        .count();
                    
                    // Count resourcepacks
                    total_items += manifest.resourcepacks.iter()
                        .filter(|r| {
                            if r.id == "default" || !r.optional {
                                true
                            } else {
                                enabled_features.contains(&r.id)
                            }
                        })
                        .count();
                    
                    // Count includes
                    total_items += manifest.include.iter()
                        .filter(|i| {
                            if i.id.is_empty() || i.id == "default" || !i.optional {
                                true
                            } else {
                                enabled_features.contains(&i.id)
                            }
                        })
                        .count();
                    
                    // Count remote includes
                    total_items += manifest.remote_include.iter()
                        .filter(|r| {
                            if r.id == "default" || !r.optional {
                                true
                            } else {
                                enabled_features.contains(&r.id)
                            }
                        })
                        .count();
                    
                    // Add overhead tasks (BUT DON'T INCLUDE THEM IN PROGRESS UNTIL THEY'RE DONE)
                    let overhead_tasks = 4;
                    total.set(total_items as i64); // Don't add overhead to total yet
                    progress.set(0);
                    status.set("Starting installation...".to_string());
                    
                    debug!("Total installation items: {}", total_items);
                    
                    // Create progress callback that properly tracks component downloads
                    let mut completed_items = 0i64;
                    let progress_callback = move || {
                        completed_items += 1;
                        progress.set(completed_items);
                        let current = completed_items;
                        let total_val = *total.read();
                        
                        let percent = if total_val > 0 {
                            ((current as f64 / total_val as f64) * 100.0) as i64
                        } else {
                            0
                        };
                        
                        // Update status based on progress
                        if percent < 30 {
                            status.set(format!("Downloading components... {}/{}", current, total_val));
                        } else if percent < 60 {
                            status.set(format!("Installing mods... {}/{}", current, total_val));
                        } else if percent < 90 {
                            status.set(format!("Configuring installation... {}/{}", current, total_val));
                        } else {
                            status.set(format!("Nearly finished... {}/{}", current, total_val));
                        }
                        
                        debug!("Progress update: {}/{} ({}%)", current, total_val, percent);
                    };
                    
                    // Run the installation
                    match installation_clone.install_or_update_with_progress(&http_client, progress_callback).await {
                        Ok(failures) => {
                            if !failures.is_empty() {
                                let summary: String = failures
                                    .iter()
                                    .map(|f| format!("[{}] {}: {}", f.kind, f.name, f.error))
                                    .collect::<Vec<_>>()
                                    .join("; ");
                                warn!(
                                    "Installation completed with {} skipped item(s): {}",
                                    failures.len(),
                                    summary
                                );
                                installation_error_clone.set(Some(format!(
                                    "Installed successfully, but {} file(s) could not be downloaded and were skipped \
                                    (see error.log in the installation folder for details).",
                                    failures.len()
                                )));
                            }
                            // NOW handle overhead tasks with proper progress updates
                            status.set("Finalizing installation...".to_string());
                            
                            // Update total to include overhead
                            let new_total = total_items as i64 + overhead_tasks;
                            total.set(new_total);
                            
                            // Complete overhead tasks one by one
                            for i in 1..=overhead_tasks {
                                progress.set(total_items as i64 + i);
                                match i {
                                    1 => status.set("Saving configuration...".to_string()),
                                    2 => status.set("Creating launcher profile...".to_string()),
                                    3 => status.set("Setting up game files...".to_string()),
                                    4 => status.set("Completing installation...".to_string()),
                                    _ => {}
                                }
                                
                                // Small delay to show each step
                                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                            }
                            
                            // FINAL: Set to 100% and mark as complete
                            progress.set(new_total);
                            status.set("Installation completed successfully!".to_string());
                            
                            debug!("Installation completed successfully, progress: {}/{}", new_total, new_total);
                            
                            // IMPORTANT: Clear session state after successful installation
                            crate::launcher::features_tab::clear_session_state(&installation_id_for_clear_async);
                            debug!("Cleared session state for installation {}", installation_id_for_clear_async);
                            
                            // Commit the installation after success
                            installation_clone.commit_installation();
                            
                            // Update installation state
                            installation_clone.installed = true;
                            installation_clone.update_available = false;
                            installation_clone.preset_update_available = false;
                            installation_clone.modified = false;
                            
                            // Update the universal version
                            if let Ok(manifest) = crate::universal::load_universal_manifest(&http_client, None).await {
                                installation_clone.universal_version = manifest.modpack_version;
                            }
                            
                            // Update preset version if needed
                            if let Some(base_preset_id) = &installation_clone.base_preset_id {
                                if let Ok(presets) = crate::preset::load_presets(&http_client, None).await {
                                    if let Some(preset) = presets.iter().find(|p| p.id == *base_preset_id) {
                                        installation_clone.base_preset_version = preset.preset_version.clone();
                                    }
                                }
                            }
                            
                            // Save the installation
                            if let Err(e) = installation_clone.save() {
                                error!("Failed to save installation: {}", e);
                                installation_error_clone.set(Some(format!("Failed to save installation: {}", e)));
                            } else {
                                debug!("Successfully saved installation state");
                                
                                // Update UI state only after successful save
                                installation_state.set(installation_clone.clone());
                                
                                // Update the installations list
                                installations.with_mut(|list| {
                                    if let Some(index) = list.iter().position(|i| i.id == installation_id) {
                                        list[index] = installation_clone;
                                    }
                                });
                                
                                // Clear modification flags
                                has_changes_clone.set(false);
                                features_modified_clone.set(false);
                                performance_modified_clone.set(false);
                                
                                debug!("Installation UI state updated successfully");
                            }
                            
                            // Wait a moment to show completion, then close
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                            debug!("Closing progress window after successful installation");
                            is_installing_clone.set(false);
                        },
                        Err(e) => {
                            error!("Installation failed: {}", e);
                            installation_error_clone.set(Some(format!("Installation failed: {}", e)));
                            status.set("Installation failed!".to_string());
                            // Don't auto-close on failure
                            // Don't clear session state on failure - let user retry with same selections
                        }
                    }
                },
                Err(e) => {
                    error!("Failed to load manifest: {}", e);
                    installation_error_clone.set(Some(format!("Failed to load manifest: {}", e)));
                    status.set("Failed to load manifest!".to_string());
                    is_installing_clone.set(false);
                }
            }
        });
    }
};


    // Handle update function
    let handle_update = {
        let mut proceed_with_update = proceed_with_update.clone();
        let installation_state = installation_state;
        let mut show_update_warning = show_update_warning;
        
        move |_| {
            // Check if this is an update (not first install)
            if installation_state.read().installed {
                // Show the update warning dialog
                show_update_warning.set(true);
            } else {
                // First install - proceed directly
                proceed_with_update();
            }
        }
    };
        
    // Button label and state
let (action_button_label, button_class, button_disabled) = {
    let current_installation = installation_state.read();
    let installed = current_installation.installed;
    let update_available = current_installation.update_available;
    let preset_update_available = current_installation.preset_update_available;
    let has_changes = *has_changes.read();
    let is_installing = *is_installing.read();
    
    if is_installing {
        ("INSTALLING...", "footer-action-button installing", true)
    } else if !installed {
        // Not installed - always allow installation
        ("INSTALL", "footer-action-button install", false)
    } else if update_available || preset_update_available {
        // Update available - show update button
        ("UPDATE", "footer-action-button update", false)
    } else if has_changes {
        // User made changes - allow modification
        ("MODIFY", "footer-action-button modify", false)
    } else {
        // Installed and up-to-date with no changes
        ("INSTALLED", "footer-action-button up-to-date", true)
    }
};  
    // Handle launch
    let handle_launch = {
        let installation_error_clone = installation_error;
        let installation_id = installation_id_for_launch.clone();
        
        move |_| {
            let mut installation_error_clone = installation_error_clone;
            let installation_id = installation_id.clone();
            
            // Create a channel to communicate back to the main thread
            let (error_tx, error_rx) = std::sync::mpsc::channel::<String>();
            
            // Launch the game
            std::thread::spawn(move || {
                match crate::launch_modpack(&installation_id) {
                    Ok(_) => {
                        debug!("Successfully launched modpack: {}", installation_id);
                    },
                    Err(e) => {
                        error!("Failed to launch modpack: {}", e);
                        let _ = error_tx.send(format!("Failed to launch modpack: {}", e));
                    }
                }
            });
            
            // Create a task to check for errors from the background thread
            spawn(async move {
                if let Ok(error_message) = error_rx.recv() {
                    installation_error_clone.set(Some(error_message));
                }
            });
        }
    };

    // Load universal manifest for features
    let universal_manifest = use_resource(move || async {
        match crate::universal::load_universal_manifest(&crate::CachedHttpClient::new(), None).await {
            Ok(manifest) => {
                debug!("Successfully loaded universal manifest for features");
                Some(manifest)
            },
            Err(e) => {
                error!("Failed to load universal manifest: {}", e);
                None
            }
        }
    });
    
    // Load presets
    let presets = use_resource(move || async {
        match crate::preset::load_presets(&crate::CachedHttpClient::new(), None).await {
            Ok(presets) => {
                debug!("Successfully loaded {} presets", presets.len());
                presets
            },
            Err(e) => {
                error!("Failed to load presets: {}", e);
                Vec::new()
            }
        }
    });

rsx! {
    div { 
        class: "installation-management-container installation-page",
            // Show progress view if installing
                if *is_installing.read() {
                    div { class: "installation-page",
                        ProgressView {
                            value: *installation_progress.read(),
                            max: *installation_total.read(),
                            status: installation_status.read().clone(),
                            title: format!("Installing {}", installation.name),
                            on_complete: Some(EventHandler::new(move |_| {
                                debug!("Progress view signaled completion");
                                is_installing.set(false);
                            }))
                        }
                    }
                } else {
                // Modern unified header
                header { class: "modern-header",
                    div { class: "header-left",
                        if let Some(icon_base64) = icon_base64 {
                            img { 
                                class: "header-logo",
                                src: "data:image/png;base64,{icon_base64}",
                                alt: "Logo"
                            }
                        }
                        
                        h1 { class: "header-title", "{installation.name}" }
                    }
                    
                    div { class: "header-center",
                        button { 
                            class: "nav-tab back-tab",
                            onclick: move |_| onback.call(()),
                            "← Back"
                        }
                        
                        button { 
                            class: if *active_tab.read() == "features" { 
                                "nav-tab active" 
                            } else { 
                                "nav-tab" 
                            },
                            onclick: move |_| active_tab.set("features"),
                            "Features"
                            
                            if *features_modified.read() {
                                span { class: "tab-modified-dot" }
                            }
                        }
                        
                        button { 
                            class: if *active_tab.read() == "performance" { 
                                "nav-tab active" 
                            } else { 
                                "nav-tab" 
                            },
                            onclick: move |_| active_tab.set("performance"),
                            "Performance"
                            
                            if *performance_modified.read() {
                                span { class: "tab-modified-dot" }
                            }
                        }
                        
                        button { 
                            class: if *active_tab.read() == "settings" { 
                                "nav-tab active" 
                            } else { 
                                "nav-tab" 
                            },
                            onclick: move |_| active_tab.set("settings"),
                            "Settings"
                        }
                    }
                    
                    div { class: "header-right",
                        button {
                            class: "header-launch-button",
                            disabled: !installation_state.read().installed || *is_installing.read(),
                            onclick: handle_launch,
                            if installation_state.read().installed {
                                "LAUNCH"
                            } else {
                                "INSTALL FIRST"
                            }
                        }
                    }
                }

                // Main content area with padding for fixed header/footer
                div { class: "installation-page-content",
                    // Error display
                    if let Some(error) = &*installation_error.read() {
                        div { class: "error-notification",
                            div { class: "error-message", "{error}" }
                            button { 
                                class: "error-close",
                                onclick: move |_| installation_error.set(None),
                                "×"
                            }
                        }
                    }

                    // Update warning dialog (if needed)
                    if *show_update_warning.read() {
                        UpdateWarningDialog {
                            onclose: move |_| show_update_warning.set(false),
                            onproceed: move |_| {
                                show_update_warning.set(false);
                                proceed_with_update();
                            },
                            installation_path: installation.installation_path.clone(), // Add this line
                        }
                    }
                    
                    // Tab content
match *active_tab.read() {
    "features" => {
        rsx! {
            FeaturesTab {
                universal_manifest: universal_manifest.read().clone().flatten(),
                presets: presets.read().clone().unwrap_or_default(),
                enabled_features: enabled_features,
                selected_preset: selected_preset,
                filter_text: filter_text,
                installation_id: installation.id.clone(),
            }
        }
    },
    "performance" => {
        rsx! {
            PerformanceTab {
                memory_allocation: memory_allocation,
                java_args: java_args,
                installation_id: installation.id.clone()
            }
        }
    },
"settings" => {
    rsx! {
        // Use the regular SettingsTab instead of EnhancedSettingsTab
        crate::launcher::SettingsTab {
            installation: installation.clone(),
            installation_id: installation_id_for_delete.clone(),
            ondelete: move |_| {
                let id_to_delete = installation_id_for_delete.clone();
                installations.with_mut(|list| {
                    list.retain(|inst| inst.id != id_to_delete);
                });
                onback.call(());
            },
            onupdate: move |updated_installation: Installation| {
                installations.with_mut(|list| {
                    if let Some(index) = list.iter().position(|i| i.id == updated_installation.id) {
                        list[index] = updated_installation.clone();
                    }
                });
            }
        }
    }
},
    _ => rsx! { div { "Unknown tab selected" } }
}
                }
                
                // Modern fixed footer
                footer { class: "modern-footer",
                    div { class: "footer-info",
                        div { class: "footer-info-item",
                            span { class: "footer-info-label", "MINECRAFT" }
                            span { class: "footer-info-value", "{installation.minecraft_version}" }
                        }
                        
                        div { class: "footer-divider" }
                        
                        div { class: "footer-info-item",
                            span { class: "footer-info-label", "FEATURES" }
                            span { class: "footer-info-value", 
                                {
                                    let enabled_count = enabled_features.read().len();
                                    if let Some(Some(manifest)) = universal_manifest.read().as_ref() {
                                        // Count actual components, not just feature IDs
                                        let mut actual_count = 0;
                                        
                                        // Count enabled mods
                                        actual_count += manifest.mods.iter()
                                            .filter(|m| enabled_features.read().contains(&m.id) || (!m.optional && m.id == "default"))
                                            .count();
                                        
                                        // Count enabled shaderpacks
                                        actual_count += manifest.shaderpacks.iter()
                                            .filter(|s| enabled_features.read().contains(&s.id) || (!s.optional && s.id == "default"))
                                            .count();
                                        
                                        // Count enabled resourcepacks
                                        actual_count += manifest.resourcepacks.iter()
                                            .filter(|r| enabled_features.read().contains(&r.id) || (!r.optional && r.id == "default"))
                                            .count();
                                        
                                        // Count enabled includes
                                        actual_count += manifest.include.iter()
                                            .filter(|i| {
                                                if i.id.is_empty() || i.id == "default" || !i.optional {
                                                    true
                                                } else {
                                                    enabled_features.read().contains(&i.id)
                                                }
                                            })
                                            .count();
                                        
                                        let total_components = manifest.mods.len() + 
                                                             manifest.shaderpacks.len() + 
                                                             manifest.resourcepacks.len() + 
                                                             manifest.include.len();
                                        
                                        format!("{}/{}", actual_count, total_components)
                                    } else {
                                        format!("{} enabled", enabled_count)
                                    }
                                }
                            }
                        }
                                                
                        if installation.update_available {
                            Fragment {
                                div { class: "footer-divider" }
                                div { class: "footer-info-item",
                                    span { class: "footer-info-label", "STATUS" }
                                    span { class: "footer-info-value", style: "color: #ffb900;", "Update Available" }
                                }
                            }
                        }
                    }
                    
                    // Action buttons container
                    div { class: "footer-actions",
                        button {
                            class: button_class,
                            disabled: button_disabled,
                            onclick: handle_update,
                            {action_button_label}
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn UpdateWarningDialog(
    onclose: EventHandler<()>,
    onproceed: EventHandler<()>,
    installation_path: PathBuf, // Add this parameter
) -> Element {
    // Function to open the installation folder
    let open_folder = move |_| {
        let path = installation_path.clone();
        
        #[cfg(target_os = "windows")]
        let result = {
            let path_str = path.to_string_lossy().replace("/", "\\");
            std::process::Command::new("explorer")
                .arg(&path_str)
                .spawn()
        };
        
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open")
            .arg(&path)
            .spawn();
            
        #[cfg(target_os = "linux")]
        let result = std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn();
            
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        let result = Err(std::io::Error::new(std::io::ErrorKind::Other, "Unsupported platform"));
        
        if let Err(e) = result {
            error!("Failed to open installation folder: {}", e);
        }
    };
    
    rsx! {
        div { class: "modal-overlay",
            div { class: "modal-container update-warning-dialog",
                div { class: "modal-header",
                    h3 { "UPDATE WARNING" }
                    button { 
                        class: "modal-close",
                        onclick: move |_| onclose.call(()),
                        "×"
                    }
                }
                
                div { class: "modal-content",
                    div { class: "warning-message",
                        p { 
                            "Updating may reset some settings, especially Wynntils settings."
                        }
                        
                        p { 
                            "To protect your Wynntils configuration:"
                        }
                        
                        ol { class: "protection-steps",
                            li { "Click 'Open Folder' below" }
                            li { "Make a backup copy of the 'wynntils' folder" }
                            li { "After updating, restore your backed-up folder if needed" }
                        }
                        
                        div { class: "warning-note",
                            p { 
                                "💡 Tip: Keep your Wynntils folder backed up regularly to avoid losing your custom settings."
                            }
                        }
                    }
                }
                
                div { class: "modal-footer",
                    button { 
                        class: "cancel-button",
                        onclick: move |_| onclose.call(()),
                        "CANCEL"
                    }
                    
                    button { 
                        class: "secondary-button open-folder-button",
                        onclick: open_folder,
                        "OPEN FOLDER"
                    }
                    
                    button { 
                        class: "update-proceed-button",
                        onclick: move |_| onproceed.call(()),
                        "PROCEED"
                    }
                }
            }
        }
    }
}

#[component]
fn ProgressView(
    value: i64,
    max: i64,
    status: String,
    title: String,
    on_complete: Option<EventHandler<()>>, // Add callback for completion
) -> Element {
    // Calculate percentage accurately
    let percentage = if max > 0 { 
        ((value as f64 / max as f64) * 100.0) as i64 
    } else { 
        0 
    };
    
    // FIXED: Proper completion detection - must be exact match and status indicates completion
    let is_complete = value >= max && max > 0 && value > 0 && (
        status.contains("completed") || 
        status.contains("Complete") || 
        status.contains("successfully") ||
        status.contains("Installation completed successfully!") ||
        (percentage >= 100 && status.contains("success"))
    );
    
    debug!("Progress: {}/{}, {}%, complete: {}, status: '{}'", value, max, percentage, is_complete, status);
    
    // Auto-close when actually complete
    if is_complete {
        use_effect({
            let on_complete = on_complete;
            move || {
                debug!("Installation complete, scheduling auto-close");
                spawn(async move {
                    // Brief delay to show completion
                    tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
                    debug!("Auto-closing progress window");
                    if let Some(callback) = on_complete {
                        callback.call(());
                    }
                });
            }
        });
    }
    
    // Display status with proper completion handling
    let display_status = if is_complete {
        "Installation completed successfully!".to_string()
    } else if status.is_empty() {
        format!("Processing... {}%", percentage)
    } else if percentage >= 99 && !is_complete {
        format!("{} - Finalizing...", status)
    } else {
        status.clone()
    };
    
    // Determine current step based on percentage
    let current_step = if is_complete {
        "complete"
    } else if percentage >= 90 {
        "finish"
    } else if percentage >= 60 {
        "configure"
    } else if percentage >= 30 {
        "extract"
    } else if percentage > 0 {
        "download"
    } else {
        "prepare"
    };
    
    let steps = [("prepare", "Prepare"),
        ("download", "Download"),
        ("extract", "Extract"),
        ("configure", "Configure"),
        ("finish", "Finish"),
        ("complete", "Complete")];
    
    // Find current step index
    let active_step_index = steps.iter().position(|(id, _)| id == &current_step).unwrap_or(0);
    
    // Determine if we should show the download warning
    let show_download_warning = !is_complete && percentage < 90;
    
    rsx! {
        div { 
            class: "progress-container",
            "data-complete": if is_complete { "true" } else { "false" },
            
            div { class: "progress-header",
                h1 { "{title}" }
            }
            
            div { class: "progress-content",
                // Step indicators
                div { class: "progress-steps",
                    for (index, (_step_id, step_label)) in steps.iter().enumerate() {
                        {
                            let step_class = if index < active_step_index {
                                "progress-step completed"
                            } else if index == active_step_index {
                                "progress-step active"
                            } else {
                                "progress-step"
                            };
                            
                            rsx! {
                                div { 
                                    class: "{step_class}",
                                    div { class: "step-dot" }
                                    div { class: "step-label", "{step_label}" }
                                }
                            }
                        }
                    }
                }
                
                // Progress bar with accurate percentage
                div { class: "progress-track",
                    div { 
                        class: "progress-bar", 
                        style: "width: {percentage}%;"
                    }
                }
                
                // Progress details
                div { class: "progress-details",
                    div { class: "progress-percentage", "{percentage}%" }
                }
                
                p { class: "progress-status", "{display_status}" }
                
                // Download warning - NEW ADDITION
                if show_download_warning {
                    div { class: "progress-warning",
                        div { class: "warning-icon", "⏱️" }
                        div { class: "warning-text",
                            p { 
                                class: "warning-main",
                                "Installation may take 5-15 minutes depending on your connection and selected features."
                            }
                            p { 
                                class: "warning-sub",
                                "Please don't close this window - the process is working even if it appears to pause briefly."
                            }
                        }
                    }
                }
                
                // Success indicator when complete
                if is_complete {
                    div { class: "completion-indicator",
                        "✓ Ready to play!"
                    }
                }
                
                // Debug info (remove in production)
                if cfg!(debug_assertions) {
                    p { 
                        class: "progress-debug",
                        style: "color: rgba(255, 255, 255, 0.5); font-size: 0.8rem; margin-top: 10px;",
                        "Debug: {value}/{max} items, percentage: {percentage}%, complete: {is_complete}"
                    }
                }
            }
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct CreditsProps {
    manifest: super::Manifest,
    enabled: Vec<String>,
    credits: Signal<bool>,
}

#[component]
fn Credits(mut props: CreditsProps) -> Element {
    rsx! {
        div { class: "credits-container",
            div { class: "credits-header",
                h1 { "{props.manifest.subtitle}" }
                button {
                    class: "close-button",
                    onclick: move |evt| {
                        props.credits.set(false);
                        evt.stop_propagation();
                    },
                    "Close"
                }
            }
            div { class: "credits-content",
                div { class: "credits-list",
                    ul {
                        for r#mod in props.manifest.mods {
                            if props.enabled.contains(&r#mod.id) {
                                li { class: "credit-item",
                                    div { class: "credit-name", "{r#mod.name}" }
                                    div { class: "credit-authors",
                                        "by "
                                        for author in &r#mod.authors {
                                            // FIXED: Proper handling of href and author name with comma
                                            {
                                                let is_last = author == r#mod.authors.last().unwrap();
                                                rsx! {
                                                    a { 
                                                        href: author.link.clone(), 
                                                        class: "credit-author",
                                                        target: "_blank",
                                                        rel: "noopener noreferrer",
                                                        {format!("{}{}", author.name, if !is_last { ", " } else { "" })}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        for shaderpack in props.manifest.shaderpacks {
                            if props.enabled.contains(&shaderpack.id) {
                                li { class: "credit-item",
                                    div { class: "credit-name", "{shaderpack.name}" }
                                    div { class: "credit-authors",
                                        "by "
                                        for author in &shaderpack.authors {
                                            // FIXED: Proper handling of href and author name with comma
                                            {
                                                let is_last = author == shaderpack.authors.last().unwrap();
                                                rsx! {
                                                    a { 
                                                        href: author.link.clone(), 
                                                        class: "credit-author",
                                                        target: "_blank",
                                                        rel: "noopener noreferrer",
                                                        {format!("{}{}", author.name, if !is_last { ", " } else { "" })}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        for resourcepack in props.manifest.resourcepacks {
                            if props.enabled.contains(&resourcepack.id) {
                                li { class: "credit-item",
                                    div { class: "credit-name", "{resourcepack.name}" }
                                    div { class: "credit-authors",
                                        "by "
                                        for author in &resourcepack.authors {
                                            // FIXED: Proper handling of href and author name with comma
                                            {
                                                let is_last = author == resourcepack.authors.last().unwrap();
                                                rsx! {
                                                    a { 
                                                        href: author.link.clone(), 
                                                        class: "credit-author",
                                                        target: "_blank",
                                                        rel: "noopener noreferrer",
                                                        {format!("{}{}", author.name, if !is_last { ", " } else { "" })}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        for include in props.manifest.include {
                            if props.enabled.contains(&include.id) && include.authors.is_some() 
                               && include.name.is_some() {
                                li { class: "credit-item",
                                    div { class: "credit-name", "{include.name.as_ref().unwrap()}" }
                                    div { class: "credit-authors",
                                        "by "
                                        for author in &include.authors.as_ref().unwrap() {
                                            // FIXED: Proper handling of href and author name with comma
                                            {
                                                let is_last = author == include.authors.as_ref().unwrap().last().unwrap();
                                                rsx! {
                                                    a { 
                                                        href: author.link.clone(), 
                                                        class: "credit-author",
                                                        target: "_blank",
                                                        rel: "noopener noreferrer",
                                                        {format!("{}{}", author.name, if !is_last { ", " } else { "" })}
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

#[component]
fn PackUninstallButton(launcher: Launcher, pack: PackName) -> Element {
    let mut hidden = use_signal(|| false);
    rsx!(
        li { hidden,
            button {
                class: "uninstall-list-item",
                onclick: move |_| {
                    uninstall(&launcher, &pack.uuid).unwrap();
                    *hidden.write() = true;
                },
                "{pack.name}"
            }
        }
    )
}

#[derive(PartialEq, Props, Clone)]
struct SettingsProps {
    config: Signal<super::Config>,
    settings: Signal<bool>,
    config_path: PathBuf,
    error: Signal<Option<String>>,
    b64_id: String,
}

#[component]
fn Settings(mut props: SettingsProps) -> Element {
    let mut vanilla = None;
    let mut multimc = None;
    let mut prism = None;
    let mut custom = None;
    let _launcher = get_launcher(&props.config.read().launcher).unwrap();
    
    match &props.config.read().launcher[..] {
        "vanilla" => vanilla = Some("true"),
        "multimc-MultiMC" => multimc = Some("true"),
        "multimc-PrismLauncher" => prism = Some("true"),
        _ => {}
    }
    if props.config.read().launcher.starts_with("custom") {
        custom = Some("true")
    }

    rsx! {
        div { class: "settings-container",
            h1 { class: "settings-title", "Launcher Settings" }  // Updated title
            form {
                id: "settings",
                class: "settings-form",
                onsubmit: move |event| {
                    props
                        .config
                        .write()
                        .launcher = event.data.values()["launcher-select"].as_value();
                    if let Err(e) = std::fs::write(
                        &props.config_path,
                        serde_json::to_vec(&*props.config.read()).unwrap(),
                    ) {
                        props.error.set(Some(format!("{:#?}", e) + " (Failed to write config!)"));
                    }
                    props.settings.set(false);
                },
                
                div { class: "setting-group",
                    label { class: "setting-label", "Minecraft Launcher:" }
                    select {
                        name: "launcher-select",
                        id: "launcher-select",
                        form: "settings",
                        class: "setting-select",
                        if super::get_minecraft_folder().is_dir() {
                            option { value: "vanilla", selected: vanilla, "Vanilla Launcher" }
                        }
                        if super::get_multimc_folder("MultiMC").is_ok() {
                            option { value: "multimc-MultiMC", selected: multimc, "MultiMC" }
                        }
                        if super::get_multimc_folder("PrismLauncher").is_ok() {
                            option {
                                value: "multimc-PrismLauncher",
                                selected: prism,
                                "Prism Launcher"
                            }
                        }
                        if custom.is_some() {
                            option {
                                value: "{props.config.read().launcher}",
                                selected: custom,
                                "Custom MultiMC"
                            }
                        }
                    }
                }
                
                CustomMultiMCButton {
                    config: props.config,
                    config_path: props.config_path.clone(),
                    error: props.error,
                    b64_id: props.b64_id.clone()
                }
                
                div { class: "settings-buttons",
                    input {
                        r#type: "submit",
                        value: "Save Changes",
                        class: "primary-button",
                        id: "save"
                    }
                }
            }
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct LauncherProps {
    config: Signal<super::Config>,
    config_path: PathBuf,
    error: Signal<Option<String>>,
    b64_id: String,
}

#[component]
fn InstallButton(
    label: String,
    disabled: bool,
    onclick: EventHandler<MouseEvent>,
    state: Option<String>  // "ready", "processing", "success", "updating", "modified"
) -> Element {
    let button_state = state.unwrap_or_else(|| "ready".to_string());
    
    rsx! {
        div { class: "install-button-container",
            div { 
                class: "button-scale-wrapper",
                style: "animation: button-scale-pulse 3s infinite alternate, button-breathe 4s infinite ease-in-out;",
                button {
                    class: "main-install-button",
                    disabled: disabled,
                    "data-state": "{button_state}",
                    onclick: move |evt| onclick.call(evt),
                    
                    span { class: "button-text", "{label}" }
                    div { class: "button-progress" }
                }
            }
        }
    }
}

#[component]
fn Launcher(mut props: LauncherProps) -> Element {
    let mut vanilla = None;
    let mut multimc = None;
    let mut prism = None;
    match &props.config.read().launcher[..] {
        "vanilla" => vanilla = Some("true"),
        "multimc-MultiMC" => multimc = Some("true"),
        "multimc-PrismLauncher" => prism = Some("true"),
        _ => {}
    }
    let has_supported_launcher = super::get_minecraft_folder().is_dir()
        || super::get_multimc_folder("MultiMC").is_ok()
        || super::get_multimc_folder("PrismLauncher").is_ok();
        
    if !has_supported_launcher {
        rsx!(NoLauncherFound {
            config: props.config,
            config_path: props.config_path,
            error: props.error,
            b64_id: props.b64_id.clone()
        })
    } else {
        rsx! {
            div { class: "launcher-container",
                h1 { class: "launcher-title", "Select Launcher" }
                form {
                    id: "launcher-form",
                    class: "launcher-form",
                    onsubmit: move |event| {
                        props
                            .config
                            .write()
                            .launcher = event.data.values()["launcher-select"].as_value();
                        props.config.write().first_launch = Some(false);
                        if let Err(e) = std::fs::write(
                            &props.config_path,
                            serde_json::to_vec(&*props.config.read()).unwrap(),
                        ) {
                            props.error.set(Some(format!("{:#?}", e) + " (Failed to write config!)"));
                        }
                    },
                    
                    div { class: "setting-group",
                        label { class: "setting-label", "Launcher:" }
                        select {
                            name: "launcher-select",
                            id: "launcher-select",
                            form: "launcher-form",
                            class: "setting-select",
                            if super::get_minecraft_folder().is_dir() {
                                option { value: "vanilla", selected: vanilla, "Vanilla" }
                            }
                            if super::get_multimc_folder("MultiMC").is_ok() {
                                option {
                                    value: "multimc-MultiMC",
                                    selected: multimc,
                                    "MultiMC"
                                }
                            }
                            if super::get_multimc_folder("PrismLauncher").is_ok() {
                                option {
                                    value: "multimc-PrismLauncher",
                                    selected: prism,
                                    "Prism Launcher"
                                }
                            }
                        }
                    }
                    
                    CustomMultiMCButton {
                        config: props.config,
                        config_path: props.config_path.clone(),
                        error: props.error,
                        b64_id: props.b64_id.clone()
                    }
                    
                    input {
                        r#type: "submit",
                        value: "Continue",
                        class: "primary-button",
                        id: "continue-button"
                    }
                }
            }
        }
    }
}

#[component]
fn CustomMultiMCButton(mut props: LauncherProps) -> Element {
    let custom_multimc = move |_evt| {
        let directory_dialog = rfd::FileDialog::new()
            .set_title("Pick root directory of desired MultiMC based launcher.")
            .set_directory(get_app_data());
        let directory = directory_dialog.pick_folder();
        if let Some(path) = directory {
            if !path.join("instances").is_dir() {
                return;
            }
            let path = path.to_str();
            if path.is_none() {
                props
                    .error
                    .set(Some(String::from("Could not get path to directory!")));
            }
            props.config.write().launcher = format!("custom-{}", path.unwrap());
            props.config.write().first_launch = Some(false);
            if let Err(e) = std::fs::write(
                &props.config_path,
                serde_json::to_vec(&*props.config.read()).unwrap(),
            ) {
                props
                    .error
                    .set(Some(format!("{:#?}", e) + " (Failed to write config!)"));
            }
        }
    };
    
    rsx!(
        button {
            class: "secondary-button custom-multimc-button",
            onclick: custom_multimc,
            r#type: "button",
            "Use custom MultiMC directory"
        }
    )
}

#[component]
fn NoLauncherFound(props: LauncherProps) -> Element {
    rsx! {
        div { class: "no-launcher-container",
            h1 { class: "no-launcher-title", "No supported launcher found!" }
            div { class: "no-launcher-message",
                p {
                    "Only Prism Launcher, MultiMC and the vanilla launcher are supported by default, other MultiMC launchers can be added using the button below."
                }
                p {
                    "If you have any of these installed then please make sure you are on the latest version of the installer, if you are, open a thread in #📂modpack-issues on the discord. Please make sure your thread contains the following information: Launcher your having issues with, directory of the launcher and your OS."
                }
            }
            CustomMultiMCButton {
                config: props.config,
                config_path: props.config_path,
                error: props.error,
                b64_id: props.b64_id.clone()
            }
        }
    }
}

fn feature_change(
    local_features: Signal<Option<Vec<String>>>,
    mut modify: Signal<bool>,
    evt: FormEvent,
    feat: &super::Feature,
    mut modify_count: Signal<i32>,
    mut enabled_features: Signal<Vec<String>>,
) {
    // Extract values first
    let enabled = match &*evt.data.value() {
        "true" => true,
        "false" => false,
        _ => panic!("Invalid bool from feature"),
    };
    
    debug!("Feature toggle changed: {} -> {}", feat.id, enabled);
    
    // Copy values we need for comparison
    let current_features = enabled_features.read().clone();
    let contains_feature = current_features.contains(&feat.id);
    let current_count = *modify_count.read();
    
    // Only update if necessary
    if enabled != contains_feature {
        debug!("Updating feature state for {}: {} -> {}", feat.id, contains_feature, enabled);
        enabled_features.with_mut(|x| {
            if enabled && !x.contains(&feat.id) {
                x.push(feat.id.clone());
            } else if !enabled {
                x.retain(|item| item != &feat.id);
            }
        });
    }
    
    // Handle modify signals in a separate step
    if let Some(local_feat) = local_features.read().as_ref() {
        let modify_res = local_feat.contains(&feat.id) != enabled;
        
        // Schedule these operations separately to avoid infinite loop warnings
        if current_count <= 1 {
            modify.set(modify_res);
        }
        
        if modify_res {
            modify_count.with_mut(|x| *x += 1);
        } else {
            modify_count.with_mut(|x| *x -= 1);
        }
    }
}

// Update the init_branch function
async fn init_branch(source: String, branch: String, launcher: Launcher, mut pages: Signal<BTreeMap<usize, TabInfo>>) -> Result<(), String> {
    debug!("Initializing modpack from source: {}, branch: {}", source, branch);
    let profile = crate::init(source.to_owned(), branch.to_owned(), launcher).await?;

    // Process manifest data for tab information
    debug!("Processing manifest tab information:");
    debug!("  subtitle: {}", profile.manifest.subtitle);
    debug!("  description length: {}", profile.manifest.description.len());

    let tab_group = if let Some(tab_group) = profile.manifest.tab_group {
        debug!("  tab_group: {}", tab_group);
        tab_group
    } else {
        debug!("  tab_group: None, defaulting to 0");
        0
    };

    // Check if this profile already exists in the tab group
    let profile_exists = pages.read().get(&tab_group)
        .is_some_and(|tab_info| tab_info.modpacks.iter()
            .any(|p| p.modpack_branch == profile.modpack_branch && p.modpack_source == profile.modpack_source));
            
    if profile_exists {
        debug!("Profile already exists in tab_group {}, skipping", tab_group);
        return Ok(());
    }

    let tab_created = pages.read().contains_key(&tab_group);
    
    // Create the tab if it doesn't exist
    if !tab_created {
        let tab_title = if let Some(ref tab_title) = profile.manifest.tab_title {
            debug!("  tab_title: {}", tab_title);
            tab_title.clone()
        } else {
            debug!("  tab_title: None, using subtitle");
            profile.manifest.subtitle.clone()
        };

        let tab_color = if let Some(ref tab_color) = profile.manifest.tab_color {
            debug!("  tab_color: {}", tab_color);
            tab_color.clone()
        } else {
            debug!("  tab_color: None, defaulting to '#320625'");
            String::from("#320625")
        };

        let tab_background = if let Some(ref tab_background) = profile.manifest.tab_background {
            debug!("  tab_background: {}", tab_background);
            tab_background.clone()
        } else {
            let default_bg = "https://raw.githubusercontent.com/Wynncraft-Overhaul/installer/master/src/assets/background_installer.png";
            debug!("  tab_background: None, defaulting to '{}'", default_bg);
            String::from(default_bg)
        };

        // Use a consistent background for settings - home background
        let settings_background = "https://raw.githubusercontent.com/Wynncraft-Overhaul/installer/master/src/assets/background_installer.png".to_string();

        // No longer storing font variables in TabInfo
        let tab_info = TabInfo {
            color: tab_color,
            title: tab_title,
            background: tab_background,
            settings_background,
            modpacks: vec![profile.clone()], // Add the profile immediately
        };
        
        pages.write().insert(tab_group, tab_info);
        debug!("Created tab_group {} with profile {}", tab_group, branch);
    } else {
        // Add the profile to an existing tab
        pages.write().entry(tab_group).and_modify(|tab_info| {
            tab_info.modpacks.push(profile.clone());
            debug!("Added profile {} to existing tab_group {}", branch, tab_group);
        });
    }

    Ok(())
}

#[derive(PartialEq, Props, Clone)]
struct VersionProps {
    installer_profile: InstallerProfile,
    error: Signal<Option<String>>,
    current_page: usize,
    tab_group: usize,
}

#[component]
fn Version(mut props: VersionProps) -> Element {
    let installer_profile = props.installer_profile.clone();


    // Add explicit debugging for initial state
    debug!("INITIAL STATE: installed={}, update_available={}", 
           installer_profile.installed, installer_profile.update_available);
    
    // Force reactivity with explicit signal declarations and consistent usage
    let mut installing = use_signal(|| false);
    let mut progress_status = use_signal(|| "".to_string());
    let mut install_progress = use_signal(|| 0);
    let mut modify = use_signal(|| false);
    let mut modify_count = use_signal(|| 0);
    let mut credits = use_signal(|| false);
    let mut expanded_features = use_signal(|| false);
    
    // Convert these to mutable signals to ensure their changes trigger rerendering
    let mut installed = use_signal(|| installer_profile.installed);
    let mut update_available = use_signal(|| installer_profile.update_available);
    let mut install_item_amount = use_signal(|| 0);

    // Debug counter to force refreshes
    let mut debug_counter = use_signal(|| 0);
    
    // IMPORTANT: Store the features collection in a signal to solve lifetime issues
    let features = use_signal(|| installer_profile.manifest.features.clone());
    
    // Clone the UUID right away to avoid ownership issues
    let _uuid = installer_profile.manifest.uuid.clone();
    
    // Add debugging to watch for signal changes
    use_effect(move || {
        debug!("SIGNAL UPDATE: installed={}, update_available={}, modify={}, credits={}, debug_counter={}",
               *installed.read(), *update_available.read(), *modify.read(), *credits.read(), *debug_counter.read());
    });

    // Use signal for enabled_features with cleaner initialization
    let mut enabled_features = use_signal(|| {
        let mut feature_list = vec!["default".to_string()];
        
        if installer_profile.installed && installer_profile.local_manifest.is_some() {
            feature_list = installer_profile.local_manifest.as_ref().unwrap().enabled_features.clone();
        } else {
            // Add default features
            for feat in &installer_profile.manifest.features {
                if feat.default {
                    feature_list.push(feat.id.clone());
                }
            }
        }

        debug!("Initialized enabled_features: {:?}", feature_list);
        feature_list
    });
    
    // Clone local_manifest to prevent ownership issues
    let mut local_features = use_signal(|| {
        installer_profile.local_manifest.as_ref().map(|manifest| manifest.enabled_features.clone())
    });
    
    // Calculate how many features to show in first row - default to 3
    let first_row_count = 3;
    
    // Feature toggle handler function
    let mut handle_feature_toggle = move |feat: super::Feature, evt: FormEvent| {
        // Extract form value
        let enabled = match &*evt.data.value() {
            "true" => true,
            "false" => false,
            _ => panic!("Invalid bool from feature"),
        };
        
        debug!("Feature toggle changed: {} -> {}", feat.id, enabled);
        
        // Update enabled_features
        enabled_features.with_mut(|feature_list| {
            if enabled {
                if !feature_list.contains(&feat.id) {
                    feature_list.push(feat.id.clone());
                    debug!("Added feature: {}", feat.id);
                }
            } else {
                feature_list.retain(|id| id != &feat.id);
                debug!("Removed feature: {}", feat.id);
            }
        });
        
        // Handle modify flag
        if let Some(local_feat) = local_features.read().as_ref() {
            let was_enabled = local_feat.contains(&feat.id);
            let is_modified = was_enabled != enabled;
            
            debug!("Feature modified check: was_enabled={}, new_state={}, is_modified={}", 
                   was_enabled, enabled, is_modified);
            
            if is_modified {
                modify_count.with_mut(|x| *x += 1);
                if *modify_count.read() > 0 {
                    modify.set(true);
                    debug!("SET MODIFY FLAG: true");
                }
            } else {
                modify_count.with_mut(|x| *x -= 1);
                if *modify_count.read() <= 0 {
                    modify.set(false);
                    debug!("SET MODIFY FLAG: false");
                }
            }
        }
        
        // Force refresh
        debug_counter.with_mut(|x| *x += 1);
    };
    
    // Installation/update submit handler
    let movable_profile = installer_profile.clone();
    let on_submit = move |_| {
        // Calculate total items to process for progress tracking
        *install_item_amount.write() = movable_profile.manifest.mods.len()
            + movable_profile.manifest.resourcepacks.len()
            + movable_profile.manifest.shaderpacks.len()
            + movable_profile.manifest.include.len();
        
        let movable_profile = movable_profile.clone();
        let movable_profile2 = movable_profile.clone();
        
        async move {
            let install = move |canceled| {
                let mut installer_profile = movable_profile.clone();
                spawn(async move {
                    if canceled {
                        return;
                    }
                    installing.set(true);
                    installer_profile.enabled_features = enabled_features.read().clone();
                    installer_profile.manifest.enabled_features = enabled_features.read().clone();
                    local_features.set(Some(enabled_features.read().clone()));

                    if !*installed.read() {
                        progress_status.set("Installing".to_string());
                        log::info!("MATCHHHHHHHHHH!");
                        match crate::install(&installer_profile, move || {
                            install_progress.with_mut(|x| *x += 1);
                        })
                        .await
                        {
                            Ok(failures) => {
                                installed.set(true);
                                debug!("SET INSTALLED: true");
                                
                                let _ = isahc::post(
                                    "https://tracking.commander07.workers.dev/track",
                                    format!(
                                        "{{
                                    \"projectId\": \"55db8403a4f24f3aa5afd33fd1962888\",
                                    \"dataSourceId\": \"{}\",
                                    \"userAction\": \"update\",
                                    \"additionalData\": {{
                                        \"old_version\": \"{}\",
                                        \"new_version\": \"{}\"
                                    }}
                                }}",
                                        installer_profile.manifest.uuid,
                                        installer_profile.local_manifest.unwrap().modpack_version,
                                        installer_profile.manifest.modpack_version
                                    ),
                                );

                                if !failures.is_empty() {
                                    let failure_lines: String = failures
                                        .iter()
                                        .map(|f| format!("[{}] {}: {}", f.kind, f.name, f.error))
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    let count = failures.len();
                                    let uuid_for_cancel = installer_profile.manifest.uuid.clone();
                                    let launcher_for_cancel = installer_profile.launcher.clone();
                                    let mut installed_for_cancel = installed;

                                    use_context::<ModalContext>().open(
                                        format!("{} file(s) could not be installed", count),
                                        rsx!(
                                            p {
                                                "The rest of the modpack installed successfully, but the following files "
                                                "could not be downloaded and were skipped. A copy of this list was saved "
                                                "to error.log in the installation's game folder."
                                            }
                                            textarea { class: "error-area", readonly: true, "{failure_lines}" }
                                            p {
                                                "You can continue using this installation as-is, or cancel to remove it."
                                            }
                                        ),
                                        true,
                                        Some(move |canceled: bool| {
                                            if canceled {
                                                if let Some(launcher) = &launcher_for_cancel {
                                                    if let Err(e) = crate::uninstall(launcher, &uuid_for_cancel) {
                                                        error!("Failed to roll back installation after cancel: {}", e);
                                                    }
                                                }
                                                installed_for_cancel.set(false);
                                                debug!("Installation cancelled after download failures; rolled back");
                                            }
                                        }),
                                    );
                                }
                            }
                            Err(e) => {
                                props.error.set(Some(
                                    format!("{:#?}", e) + " (Failed to update modpack!)",
                                ));
                                installing.set(false);
                                return;
                            }
                        }
                        update_available.set(false);
                        debug!("SET UPDATE_AVAILABLE: false");
                    } else if *modify.read() {
                        progress_status.set("Modifying".to_string());
                        match super::update(&installer_profile, move || {
                            install_progress.with_mut(|x| *x += 1);
                        })
                        .await
                        {
                            Ok(failures) => {
                                let _ = isahc::post(
                                    "https://tracking.commander07.workers.dev/track",
                                    format!(
                                        "{{
                                    \"projectId\": \"55db8403a4f24f3aa5afd33fd1962888\",
                                    \"dataSourceId\": \"{}\",
                                    \"userAction\": \"modify\",
                                    \"additionalData\": {{
                                        \"features\": {:?}
                                    }}
                                }}",
                                        installer_profile.manifest.uuid,
                                        installer_profile.manifest.enabled_features
                                    ),
                                );

                                if !failures.is_empty() {
                                    // Note: unlike a fresh install, "cancel" here isn't offered as a
                                    // destructive rollback — this operates on an already-installed,
                                    // working pack, so we just surface what was skipped.
                                    let failure_lines: String = failures
                                        .iter()
                                        .map(|f| format!("[{}] {}: {}", f.kind, f.name, f.error))
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    let count = failures.len();

                                    use_context::<ModalContext>().open(
                                        format!("{} file(s) could not be updated", count),
                                        rsx!(
                                            p {
                                                "The rest of the modpack updated successfully, but the following files "
                                                "could not be downloaded and were skipped. A copy of this list was saved "
                                                "to error.log in the installation's game folder."
                                            }
                                            textarea { class: "error-area", readonly: true, "{failure_lines}" }
                                        ),
                                        false,
                                        Option::<fn(bool)>::None,
                                    );
                                }
                            }
                            Err(e) => {
                                props.error.set(Some(
                                    format!("{:#?}", e) + " (Failed to modify modpack!)",
                                ));
                                installing.set(false);
                                return;
                            }
                        }
                        modify.set(false);
                        debug!("RESET MODIFY: false");
                        modify_count.set(0);
                        update_available.set(false);
                        debug!("SET UPDATE_AVAILABLE: false");
                    }
                    installing.set(false);
                    
                    // Force refresh
                    debug_counter.with_mut(|x| *x += 1);
                });
            };

            if let Some(contents) = movable_profile2.manifest.popup_contents {
                use_context::<ModalContext>().open(
                    movable_profile2.manifest.popup_title.unwrap_or_default(),
                    rsx!(div {
                        dangerous_inner_html: "{contents}",
                    }),
                    true,
                    Some(install),
                )
            } else {
                install(false);
            }
        }
    };

    // Button label based on state
    let _button_label = if !*installed.read() {
        debug!("Button state: Install");
        "Install"
    } else if *update_available.read() {
        debug!("Button state: Update");
        "Update"
    } else if *modify.read() {
        debug!("Button state: Modify");
        "Modify"
    } else {
        debug!("Button state: Modify (default)");
        "Modify"
    };
    
    // Button disable logic
    let install_disable = *installed.read() && !*update_available.read() && !*modify.read();
    debug!("Button disabled: {}", install_disable);
    
    // Pre-build feature cards to avoid nested RSX macros
    let feature_cards_content = {
        // Filter features
        let features_list = features.read();
        let visible_features: Vec<_> = features_list.iter()
            .filter(|f| !f.hidden)
            .collect();
        
        // Calculate whether to show expand button
        let show_expand_button = visible_features.len() > first_row_count;
        
        let first_row_cards = visible_features.iter().take(first_row_count).map(|feat| {
            let is_enabled = enabled_features.read().contains(&feat.id);
            let feat_clone = (*feat).clone();
            
            rsx! {
                div { 
                    class: if is_enabled { "feature-card feature-enabled" } else { "feature-card feature-disabled" },
                    div { class: "feature-card-header",
                        h3 { class: "feature-card-title", "{feat.name}" }
                    }
                    
                    if let Some(description) = &feat.description {
                        div { class: "feature-card-description", "{description}" }
                    }
                    
                    label {
                        class: if is_enabled { "feature-toggle-button enabled" } else { "feature-toggle-button disabled" },
                        input {
                            r#type: "checkbox",
                            name: "{feat.id}",
                            checked: if is_enabled { Some("true") } else { None },
                            onchange: move |evt| handle_feature_toggle(feat_clone.clone(), evt),
                            style: "display: none;"
                        }
                        if is_enabled { "Enabled" } else { "Disabled" }
                    }
                }
            }
        }).collect::<Vec<_>>();
        
        // Additional features (shown only when expanded)
        let additional_cards = if *expanded_features.read() {
            visible_features.iter().skip(first_row_count).map(|feat| {
                let is_enabled = enabled_features.read().contains(&feat.id);
                let feat_clone = (*feat).clone();
                
                rsx! {
                    div { 
                        class: if is_enabled { "feature-card feature-enabled" } else { "feature-card feature-disabled" },
                        div { class: "feature-card-header",
                            h3 { class: "feature-card-title", "{feat.name}" }
                        }
                        
                        if let Some(description) = &feat.description {
                            div { class: "feature-card-description", "{description}" }
                        }
                        
                        label {
                            class: if is_enabled { "feature-toggle-button enabled" } else { "feature-toggle-button disabled" },
                            input {
                                r#type: "checkbox",
                                name: "{feat.id}",
                                checked: if is_enabled { Some("true") } else { None },
                                onchange: move |evt| handle_feature_toggle(feat_clone.clone(), evt),
                                style: "display: none;"
                            }
                            if is_enabled { "Enabled" } else { "Disabled" }
                        }
                    }
                }
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        
        // Expand button
        let expand_button = if show_expand_button {
            vec![rsx! {
                div { class: "features-expand-container",
                    button {
                        class: "features-expand-button",
                        onclick: move |_| {
                            let current_state = *expanded_features.read();
                            expanded_features.set(!current_state);
                            debug!("Toggled expanded features: {}", !current_state);
                        },
                        if *expanded_features.read() {
                            "Collapse Features"
                        } else {
                            {format!("Show {} More Features", visible_features.len() - first_row_count)}
                        }
                    }
                }
            }]
        } else {
            Vec::new()
        };
        
        // Combine all components
        let mut all_components = Vec::new();
        all_components.extend(first_row_cards);
        all_components.extend(additional_cards);
        all_components.extend(expand_button);
        
        all_components
    };
    
    rsx! {
        if *installing.read() {
            ProgressView {
                value: *install_progress.read(),
                max: *install_item_amount.read() as i64,
                title: installer_profile.manifest.subtitle.clone(),
                status: progress_status.read().clone()
            }
        } else if *credits.read() {
            Credits {
                manifest: installer_profile.manifest.clone(),
                enabled: enabled_features.read().clone(),
                credits
            }
        } else {
            div { class: "version-container",
                "<!-- debug counter: {*debug_counter.read()} -->",
                
                form { onsubmit: on_submit,
                    // Header section with title and subtitle
                    div { class: "content-header",
                        h1 { "{installer_profile.manifest.subtitle}" }
                    }
                    
                    // Description section
                    div { class: "content-description",
                        dangerous_inner_html: "{installer_profile.manifest.description}",
                        
                        // Credits link
                        div { class: "credits-link-container", style: "text-align: center; margin: 15px 0;",
                            a {
                                class: "credits-button",
                                onclick: move |evt| {
                                    debug!("Credits clicked");
                                    credits.set(true);
                                    debug!("SET CREDITS: true");
                                    evt.stop_propagation();
                                },
                                "VIEW CREDITS"
                            }
                        }
                    }
                    
                    // Expandable Features Section
                    div { class: "features-section",
                        h2 { class: "features-heading", "OPTIONAL FEATURES" }
                        
                        // Feature cards container - using pre-built content instead of nested RSX
                        div { class: "feature-cards-container",
                            {feature_cards_content.into_iter()}
                        }
                    }
                }
            }
        }
    }
}

/// New header component with tabs - updated to display tab groups 1-3 in main row
#[component]
fn AppHeader(
    installations: Signal<Vec<Installation>>,
    current_installation_id: Signal<Option<String>>,
    on_select_installation: EventHandler<String>,
    on_go_home: EventHandler<()>,
    on_open_settings: EventHandler<()>,
    show_installation_tabs: bool,
) -> Element {
    let icon_base64 = {
        use base64::{Engine, engine::general_purpose::STANDARD};
        STANDARD.encode(include_bytes!("assets/icon.png"))
    };
    
    // For home page - show simple header with home/installations navigation
    if !show_installation_tabs {
        return rsx! {
            header { class: "modern-header",
                div { class: "header-left",
                    img { 
                        class: "header-logo", 
                        src: "data:image/png;base64,{icon_base64}",
                        alt: "Logo"
                    }
                    h1 { class: "header-title", "MAJESTIC OVERHAUL" }
                }
                
                div { class: "header-center",
                    button { 
                        class: if current_installation_id.read().is_none() { 
                            "nav-tab active" 
                        } else { 
                            "nav-tab" 
                        },
                        onclick: move |_| on_go_home.call(()),
                        "Home"
                    }
                    
                    // Direct installation tabs - CHANGED FROM take(3) TO take(2)
                    for installation in installations().iter().take(2) {
                        {
                            let id = installation.id.clone();
                            let name = installation.name.clone();
                            let is_active = current_installation_id.read().as_ref() == Some(&id);
                            
                            rsx! {
                                button {
                                    class: if is_active { "nav-tab active" } else { "nav-tab" },
                                    onclick: move |_| on_select_installation.call(id.clone()),
                                    "{name}"
                                }
                            }
                        }
                    }
                    
                    // More dropdown if needed
                    if installations().len() > 1 {
                        div { class: "dropdown",
                            button { class: "nav-tab", "More ▼" }
                            div { class: "dropdown-content",
                                // CHANGED FROM skip(3) TO skip(2)
                                for installation in installations().iter().skip(2) {
                                    {
                                        let id = installation.id.clone();
                                        let name = installation.name.clone();
                                        let is_active = current_installation_id.read().as_ref() == Some(&id);
                                        
                                        rsx! {
                                            button {
                                                class: if is_active { "dropdown-item active" } else { "dropdown-item" },
                                                onclick: move |_| on_select_installation.call(id.clone()),
                                                "{name}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // New installation button
                    button { 
                        class: "nav-tab",
                        onclick: move |_| on_select_installation.call("new".to_string()),
                        "+"
                    }
                }
                
                div { class: "header-right",
                    button { 
                        class: "nav-tab",
                        onclick: move |_| on_open_settings.call(()),
                        "Launcher"
                    }
                }
            }
        };
    }
    
    // If we need installation management tabs (this path might not be used anymore)
    rsx! { Fragment {} }
}

#[derive(Debug, Clone)]
pub struct AppProps {
    pub branches: Vec<super::GithubBranch>,
    pub modpack_source: String,
    pub config: super::Config,
    pub config_path: PathBuf,
    pub installations: Vec<Installation>,
}

// Fixed app function
pub fn app() -> Element {
    let props = use_context::<AppProps>();
    let css = include_str!("assets/style.css");
    
    // State management
    let config = use_signal(|| props.config);
    let mut settings = use_signal(|| false);
    let mut error_signal = use_signal(|| Option::<String>::None);
    let mut manifest_error = use_signal(|| Option::<ManifestError>::None);
    
    // Installation handling
    let mut current_installation_id = use_signal(|| Option::<String>::None);
    let mut installations = use_signal(|| props.installations.clone());

    // Get launcher configuration
        let has_launcher = match get_launcher(&config.read().launcher) {
            Ok(_) => true,
            Err(e) => {
                error!("Failed to load launcher: {} - {}", config.read().launcher, e);
                false
            }
        };

    // Load universal manifest with error handling
    let _has_launcher_copy = has_launcher;

    let config_clone = config;
    let manifest_error_clone = manifest_error;
    let universal_manifest = use_resource(move || {
        let config = config_clone;
        let mut manifest_error = manifest_error_clone;
        async move {
            // Clone the launcher string
            let launcher_str = config.read().launcher.clone();
            
            // Now use the string value with get_launcher
            let launcher = get_launcher(&launcher_str).ok();
            
            let launcher_available = launcher.is_some();
            if !launcher_available {
                return None;
            }
            
            debug!("Loading universal manifest...");
        match crate::universal::load_universal_manifest(&CachedHttpClient::new(), None).await {
            Ok(manifest) => {
                debug!("Successfully loaded universal manifest: {}", manifest.name);
                Some(manifest)
            },
            Err(e) => {
                error!("Failed to load universal manifest: {}", e);
                spawn(async move {
                    manifest_error.set(Some(e.clone()));
                });
                None
            }
        }
    } // <- Make sure this closing brace exists
}); // <- And this closing parenthesis and semicolon
    
    // Load changelog
let changelog = use_resource(move || async {
    match crate::changelog::fetch_changelog("Olinus10/installer-test/master", &CachedHttpClient::new()).await {
        Ok(changelog) => {
            debug!("Successfully loaded changelog with {} entries", changelog.entries.len());
            Some(changelog)
        },
        Err(e) => {
            error!("Failed to load changelog: {}", e);
            None
        }
    }
});

let mut changelog_signal = use_signal(|| None::<ChangelogData>);
use_effect(move || {
    if let Some(Some(changelog_data)) = changelog.read().as_ref() {
        debug!("App: Setting changelog_signal with {} entries", changelog_data.entries.len());
        if let Some(ref config) = changelog_data.homepage_config {
            debug!("App: Changelog has homepage_config - button: '{}' -> '{}'", 
                   config.footer_button.text, config.footer_button.link);
        }
        changelog_signal.set(Some(changelog_data.clone()));
    } else {
        debug!("App: No changelog data to set");
    }
});
    
    use_effect(move || {
    let installations = installations;
    let http_client = CachedHttpClient::new();
    
    spawn(async move {
        // Check for updates on startup
        for mut installation in installations.read().clone() {
            if installation.installed {
                if let Ok(presets) = crate::preset::load_presets(&http_client, None).await {
                    let _ = installation.check_for_updates(&http_client, &presets).await;
                }
            }
        }
    });
});

    // Modal context for popups
    let mut modal_context = use_context_provider(ModalContext::default);
    
    // Show error modal if error exists
    if let Some(e) = error_signal() {
        modal_context.open("Error", rsx! {
            p {
                "The installer encountered an error. If the problem persists, please report it in #📂modpack-issues on Discord."
            }
            textarea { class: "error-area", readonly: true, "{e}" }
        }, false, Some(move |_| error_signal.set(None)));
    }

    // Build CSS content
    let css_content = css
        .replace("<BG_COLOR>", "#320625")
        .replace("<BG_IMAGE>", "https://raw.githubusercontent.com/Wynncraft-Overhaul/installer/master/src/assets/background_installer.png")
        .replace("<SECONDARY_FONT>", "\"HEADER_FONT\"")
        .replace("<PRIMARY_FONT>", "\"REGULAR_FONT\"");
    
// Add custom category styles
let category_styles = include_str!("assets/category-styles.css");
let feature_styles = include_str!("assets/expanded-feature-styles.css");
let preset_styles = include_str!("assets/preset-styles.css");
let search_styles = include_str!("assets/search-results-styles.css");
let modal_styles = include_str!("assets/modal-styles.css");
let installation_header_styles = include_str!("assets/installation-header-styles.css");
//let file_tree_styles = include_str!("assets/file-tree-styles.css");

// Combine all CSS files
let complete_css = format!("{}\n{}\n{}\n{}\n{}\n{}\n{}", 
    css_content, 
    category_styles, 
    feature_styles, 
    preset_styles, 
    search_styles,
    modal_styles,
    installation_header_styles
);

    let mut modal_context = use_context_provider(ModalContext::default);
    
    // Show error modal if error exists
    if let Some(e) = error_signal() {
        modal_context.open("Error", rsx! {
            p {
                "The installer encountered an error. If the problem persists, please report it in #📂modpack-issues on Discord."
            }
            textarea { class: "error-area", readonly: true, "{e}" }
        }, false, Some(move |_| error_signal.set(None)));
    }

    // [Keep existing CSS building code...]

    // NEW: Determine if we should show the header and what type
    let show_header = !config.read().first_launch.unwrap_or(true) && has_launcher && !settings();
    let is_on_installation_page = current_installation_id.read().is_some() && 
                                 current_installation_id.read().as_ref().is_some_and(|id| id != "new");

    // Create header component based on current page
    let header_component = if show_header {
        Some(rsx! {
            AppHeader {
                installations: installations,
                current_installation_id: current_installation_id,
                show_installation_tabs: false, // NEW: Never show installation tabs in main header
                on_select_installation: move |id: String| {
                    if id == "new" {
                        current_installation_id.set(Some(id));
                    } else {
                        current_installation_id.set(Some(id));
                    }
                },
                on_go_home: move |_| {
                    current_installation_id.set(None);
                },
                on_open_settings: move |_| {
                    settings.set(true);
                }
            }
        })
    } else {
        None
    };

    // Determine what content to show
    let main_content = if settings() {
        // Settings screen
        rsx! {
            Settings {
                config,
                settings,
                config_path: props.config_path.clone(),
                error: error_signal,
                b64_id: URL_SAFE_NO_PAD.encode(props.modpack_source)
            }
        }
    } else if config.read().first_launch.unwrap_or(true) || !has_launcher {
        // Launcher selection for first launch
        rsx! {
            Launcher {
                config,
                config_path: props.config_path.clone(),
                error: error_signal,
                b64_id: URL_SAFE_NO_PAD.encode(props.modpack_source)
            }
        }
    } else if universal_manifest.read().is_none() && has_launcher {
        // Loading screen while universal manifest loads
        rsx! {
            div { class: "loading-container",
                div { class: "loading-spinner" }
                div { class: "loading-text", "Loading modpack information..." }
                p { class: "loading-info", "This may take a moment. Please wait..." }
            }
        }
    } else {
        // Main content based on current state
        if current_installation_id.read().is_none() {
            // Home page - show installations or welcome screen
            rsx! {
                HomePage {
                    installations,
                    error_signal: error_signal,
                    changelog: changelog_signal,
                    current_installation_id: current_installation_id,
                }
            }
        } else if current_installation_id.read().as_ref().is_some_and(|id| id == "new") {
            // New installation flow
            rsx! {
                SimplifiedInstallationWizard {
                    onclose: move |_| {
                        current_installation_id.set(None);
                    },
                    oncreate: move |new_installation: Installation| {
                        installations.with_mut(|list| {
                            list.insert(0, new_installation.clone());
                        });
                        current_installation_id.set(Some(new_installation.id));
                        
                    }
                }
            }
        } else {
            // Specific installation management page - NO REGULAR HEADER HERE
            let back_handler = EventHandler::new(move |_| {
                current_installation_id.set(None);
            });
            
            let id = current_installation_id.read().as_ref().unwrap().clone();
            
            rsx! {
                InstallationManagementPage {
                    installation_id: id,
                    onback: back_handler,
                    installations: installations
                }
            }
        }
    };

    // Combine components for final render
    rsx! {
        div {
            style { {complete_css} }
            Modal {}
            BackgroundParticles {}
            
            // Show header ONLY when NOT on installation page
            if let Some(header) = header_component {
                if !is_on_installation_page {
                    {header}
                }
            }

            div { 
                class: if is_on_installation_page {
                    "main-container installation-page" // Different class for installation pages
                } else {
                    "main-container"
                },
                {main_content}
            }
            
            // Only show footer on home page (not installation pages)
if !settings() && current_installation_id.read().is_none() && 
   !config.read().first_launch.unwrap_or(true) && has_launcher {
    Footer { changelog: changelog_signal() }  
}
            
            // Add manifest error display outside of the main container
            if let Some(error) = manifest_error() {
                ManifestErrorDisplay {
                    error: error.message.clone(),
                    error_type: format!("{}", error.error_type),
                    file_name: error.file_name.clone(),
                    onclose: move |_| manifest_error.set(None),
                    onreport: move |_| {
                        let _ = open_url("https://discord.com/channels/778965021656743966/1234506784626970684");
                    }
                }
            }
        }
    }
}
