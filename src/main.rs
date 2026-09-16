#![cfg_attr(
    all(target_os = "windows", not(debug_assertions),),
    windows_subsystem = "windows"
)]
use async_trait::async_trait;
use base64::{engine, Engine};
use cached::proc_macro::cached;
use cached::SizedCache;
use chrono::{DateTime, Utc};
use dioxus::desktop::tao::window::Icon;
use dioxus::prelude::LaunchBuilder;
use dioxus::desktop::{Config as DioxusConfig, LogicalSize, WindowBuilder};
use futures::StreamExt;
use image::{DynamicImage, ImageFormat};
use isahc::config::RedirectPolicy;
use isahc::http::{HeaderMap, HeaderValue, StatusCode};
use isahc::prelude::Configurable;
use isahc::{AsyncBody, AsyncReadResponseExt, HttpClient, ReadResponseExt, Request, Response};
use log::{error, info, warn, debug};
use platform_info::{PlatformInfo, PlatformInfoAPI, UNameAPI};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use simplelog::{
    ColorChoice, CombinedLogger, Config as LogConfig, LevelFilter, TermLogger, TerminalMode,
    WriteLogger,
};
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::fs::File;
use std::thread::sleep;
use std::time::Duration;
use std::{backtrace::Backtrace, panic};
use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::SystemTime,
};
use std::boxed::Box;
use std::pin::Pin;
use futures::Future;
use lazy_static::lazy_static;

mod gui;
mod launcher;
mod changelog;
mod installation;
mod preset;
mod universal;
mod backup;

// Update your re-exports
pub use launcher::{launch_modpack, update_jvm_args, get_jvm_args};
pub use installation::{Installation, get_active_installation, load_all_installations};
pub use preset::{Preset, load_presets};
pub use universal::{UniversalManifest, load_universal_manifest, ModComponent};
pub use universal::{ManifestError, ManifestErrorType};
pub use changelog::{
    Changelog, ChangelogEntry, HomePageStats, FooterButton, HomePageConfig, fetch_changelog
};


// CORRECTED: Use direct exports from backup module
pub use backup::{
    BackupConfig, BackupType, BackupMetadata, BackupProgress, BackupItem,
    RollbackManager, RollbackOption, format_bytes,
    calculate_directory_size, count_files_recursive,
    create_zip_archive, extract_zip_archive
};


const CURRENT_MANIFEST_VERSION: i32 = 3;
const GH_API: &str = "https://api.github.com/repos/";
const GH_RAW: &str = "https://raw.githubusercontent.com/";
const CONCURRENCY: usize = 14;
const ATTEMPTS: usize = 3;
const WAIT_BETWEEN_ATTEMPTS: Duration = Duration::from_secs(20);
const REPO: &str = "Wynncraft-Overhaul/majestic-overhaul/";

const DEFAULT_UNIVERSAL_URL: &str = "https://raw.githubusercontent.com/Wynncraft-Overhaul/majestic-overhaul/master/universal.json";
const DEFAULT_PRESETS_URL: &str = "https://raw.githubusercontent.com/Wynncraft-Overhaul/majestic-overhaul/master/presets.json";
const DEFAULT_CHANGELOG_URL: &str = "https://raw.githubusercontent.com/Wynncraft-Overhaul/majestic-overhaul/master/changelog.json";

fn validate_safe_path(base: &Path, user_path: &str) -> Result<PathBuf, String> {
    // Reject obvious traversal attempts
    if user_path.contains("..") || user_path.starts_with('/') || user_path.contains('\0') {
        return Err("Invalid path detected".to_string());
    }
    
    let target = base.join(user_path);
    
    // Canonicalize and verify it's still within base directory
    match target.canonicalize() {
        Ok(canonical) => {
            match base.canonicalize() {
                Ok(canonical_base) => {
                    if canonical.starts_with(canonical_base) {
                        Ok(canonical)
                    } else {
                        Err("Path traversal attempt detected".to_string())
                    }
                },
                Err(_) => Err("Invalid base directory".to_string())
            }
        },
        Err(_) => {
            // If canonicalize fails, do a basic check
            if target.starts_with(base) {
                Ok(target)
            } else {
                Err("Invalid path".to_string())
            }
        }
    }
}

pub struct TrackingClient {
    http_client: CachedHttpClient,
    project_id: String,
    enabled: bool,
}

impl TrackingClient {
    pub fn new(project_id: String) -> Self {
        Self {
            http_client: CachedHttpClient::new(),
            project_id,
            enabled: true,
        }
    }

    pub async fn track_event(&self, action: &str, data_source_id: &str, additional_data: serde_json::Value) -> Result<(), String> {
        if !self.enabled {
            debug!("Tracking disabled, skipping event: {}", action);
            return Ok(());
        }

        let payload = serde_json::json!({
            "projectId": self.project_id,
            "dataSourceId": data_source_id,
            "userAction": action,
            "additionalData": additional_data,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "version": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS
        });

        let request = isahc::Request::post("https://tracking.commander07.workers.dev/track")
            .header("Content-Type", "application/json")
            .header("User-Agent", format!("wynncraft-overhaul-installer/{}", env!("CARGO_PKG_VERSION")))
            .body(payload.to_string())
            .map_err(|e| format!("Failed to create tracking request: {}", e))?;

        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.http_client.http_client.send_async(request)
        ).await {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    debug!("Successfully tracked event: {}", action);
                } else {
                    warn!("Tracking server returned status: {}", response.status());
                }
                Ok(())
            },
            Ok(Err(e)) => {
                warn!("Failed to send tracking event: {}", e);
                Ok(()) // Don't fail the main operation
            },
            Err(_) => {
                warn!("Tracking request timed out");
                Ok(()) // Don't fail the main operation
            }
        }
    }
}

// Global tracking client
lazy_static! {
    static ref TRACKING_CLIENT: std::sync::Mutex<Option<TrackingClient>> = 
        std::sync::Mutex::new(None);
}

pub fn init_tracking() {
    let client = TrackingClient::new("55db8403a4f24f3aa5afd33fd1962888".to_string());
    if let Ok(mut tracker) = TRACKING_CLIENT.lock() {
        *tracker = Some(client);
    }
}

pub async fn track_event(action: &str, data_source_id: &str, additional_data: serde_json::Value) {
    if let Ok(tracker) = TRACKING_CLIENT.lock() {
        if let Some(client) = tracker.as_ref() {
            if let Err(e) = client.track_event(action, data_source_id, additional_data).await {
                debug!("Tracking failed: {}", e);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PackName {
    name: String,
    uuid: String,
}

fn default_id() -> String {
    String::from("default")
}

fn default_enabled_features() -> Vec<String> {
    vec![default_id()]
}

fn default_hidden() -> bool {
    false
}

fn default_false() -> bool {
    false
}

macro_rules! add_headers {
    ($items:expr, $($headers:expr),*) => {
        $items.$(header($headers.next().unwrap().0, $headers.next().unwrap().1))*
    };
}

#[derive(Debug)]
struct CachedResponse {
    resp: Response<AsyncBody>,
    bytes: Vec<u8>,
}

fn resp_rebuilder(resp: &Response<AsyncBody>, bytes: &Vec<u8>) -> Response<AsyncBody> {
    let builder = Response::builder()
        .status(resp.status())
        .version(resp.version());
    let builder = add_headers!(builder, resp.headers().into_iter());
    builder.body(AsyncBody::from(bytes.to_owned())).unwrap()
}

impl CachedResponse {
    async fn new(mut resp: Response<AsyncBody>) -> Self {
        let bytes = resp.bytes().await.unwrap();

        Self {
            resp: resp_rebuilder(&resp, &bytes),
            bytes,
        }
    }
}

impl Clone for CachedResponse {
    fn clone(&self) -> Self {
        Self {
            resp: resp_rebuilder(&self.resp, &self.bytes),
            bytes: self.bytes.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CachedHttpClient {
    pub http_client: HttpClient,
}

impl Default for CachedHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CachedHttpClient {
    pub fn new() -> CachedHttpClient {
        CachedHttpClient {
            http_client: build_http_client(),
        }
    }

    pub async fn get_async<T: Into<String> + Clone + Debug>(
        &self,
        url: T,
    ) -> Result<Response<AsyncBody>, isahc::Error> {
        let mut err = None;
        for _ in 0..ATTEMPTS {
            let resp = get_cached(&self.http_client, url.clone().into()).await;
            match resp {
                Ok(v) => return Ok(v.resp),
                Err(v) => err = Some(v),
            }
            warn!("Failed to get '{url:?}', returned '{err:#?}'. Retrying!");
            sleep(WAIT_BETWEEN_ATTEMPTS);
        }
        error!("Failed to get '{url:?}', returned '{err:#?}'.");
        Err(err.unwrap()) // unwrap can't fail
    }

    pub async fn get_nocache<T: Into<String> + Clone>(
        &self,
        url: T,
    ) -> Result<Response<AsyncBody>, isahc::Error> {
        let mut err = None;
        for _ in 0..ATTEMPTS {
            let resp = self.http_client.get_async(url.clone().into()).await;
            match resp {
                Ok(v) => return Ok(v),
                Err(v) => err = Some(v),
            }
            sleep(WAIT_BETWEEN_ATTEMPTS);
        }
        Err(err.unwrap()) // unwrap can't fail
    }

    pub async fn with_headers<T: Into<String>>(
        &self,
        url: T,
        headers: &[(&str, &str)],
    ) -> Result<Response<AsyncBody>, isahc::Error> {
        self.http_client
            .send_async(
                add_headers!(Request::get(url.into()), headers.iter())
                    .body(())
                    .unwrap(),
            )
            .await
    }
}

#[cached(
    ty = "SizedCache<String, Result<CachedResponse, isahc::Error>>",
    create = "{ SizedCache::with_size(100) }",
    convert = r#"{ format!("{}", url) }"#
)]
async fn get_cached(http_client: &HttpClient, url: String) -> Result<CachedResponse, isahc::Error> {
    let resp = http_client.get_async(url).await;
    match resp {
        Ok(val) => Ok(CachedResponse::new(val).await),
        Err(err) => Err(err),
    }
}

fn build_http_client() -> HttpClient {
    HttpClient::builder()
        .redirect_policy(RedirectPolicy::Limit(5))
        .default_headers(&[(
            "User-Agent",
            concat!("wynncraft-overhaul/installer/", env!("CARGO_PKG_VERSION")),
        )])
        .build()
        .unwrap()
}

#[async_trait]
trait Downloadable {
    async fn download(
        &self,
        modpack_root: &Path,
        loader_type: &str,
        http_client: &CachedHttpClient,
    ) -> Result<PathBuf, DownloadError>;

    fn new(
        name: String,
        source: String,
        location: String,
        version: String,
        path: Option<PathBuf>,
        id: String,
        authors: Vec<Author>,
    ) -> Self;
    fn get_name(&self) -> &String;
    fn get_location(&self) -> &String;
    fn get_version(&self) -> &String;
    fn get_path(&self) -> &Option<PathBuf>;
    fn get_id(&self) -> &String;
    fn get_source(&self) -> &String;
    fn get_authors(&self) -> &Vec<Author>;
}


#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
struct Include {
    location: String,
    #[serde(default = "default_id")]
    id: String,
    name: Option<String>,
    authors: Option<Vec<Author>>,
    #[serde(default = "default_false")]
    optional: bool,
    #[serde(default = "default_false")]
    default_enabled: bool,
    #[serde(default = "default_false")]
    ignore_update: bool,
    // NEW: Add can_reset field
    #[serde(default = "default_false")]
    pub can_reset: bool,
    // NEW: Add missing fields for consistency
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub dependencies: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
struct Config {
    launcher: String,
    first_launch: Option<bool>, // option for backwars compatibiliy
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Author {
    pub name: String,
    pub link: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
struct Included {
    md5: String,
    files: Vec<String>,
}

macro_rules! gen_downloadble_impl {
    ($item:ty, $type:literal) => {
        #[async_trait]
        impl Downloadable for $item {
            async fn download(
                &self,
                modpack_root: &Path,
                loader_type: &str,
                http_client: &CachedHttpClient,
            ) -> Result<PathBuf, DownloadError> {
                debug!("Downloading: {self:#?}");
                let res = match self.source.as_str() {
                    "modrinth" => {
                        download_from_modrinth(self, modpack_root, loader_type, $type, http_client)
                            .await
                    }
                    "ddl" => download_from_ddl(self, modpack_root, $type, http_client).await,
                    "mediafire" => {
                        download_from_mediafire(self, modpack_root, $type, http_client).await
                    }
                    _ => panic!("Unsupported source '{}'!", self.source.as_str()),
                };
                debug!("Downloaded '{}' with result: {:#?}", self.get_name(), res);
                res
            }

            fn new(
                name: String,
                source: String,
                location: String,
                version: String,
                path: Option<PathBuf>,
                id: String,
                authors: Vec<Author>,
            ) -> Self {
                Self {
                    name,
                    source,
                    location,
                    version,
                    path,
                    id,
                    authors,
                    ignore_update: false, // Add this line with default value
                }
            }

            fn get_name(&self) -> &String {
                &self.name
            }
            fn get_location(&self) -> &String {
                &self.location
            }
            fn get_version(&self) -> &String {
                &self.version
            }
            fn get_path(&self) -> &Option<PathBuf> {
                &self.path
            }
            fn get_id(&self) -> &String {
                &self.id
            }
            fn get_source(&self) -> &String {
                &self.source
            }
            fn get_authors(&self) -> &Vec<Author> {
                &self.authors
            }
        }
    };
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
struct Mod {
    name: String,
    source: String,
    location: String,
    version: String,
    path: Option<PathBuf>,
    #[serde(default = "default_id")]
    id: String,
    authors: Vec<Author>,
    // NEW: Add ignore_update support
    #[serde(default = "default_false")]
    ignore_update: bool,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
struct Shaderpack {
    name: String,
    source: String,
    location: String,
    version: String,
    path: Option<PathBuf>,
    #[serde(default = "default_id")]
    id: String,
    authors: Vec<Author>,
    // NEW: Add ignore_update support
    #[serde(default = "default_false")]
    ignore_update: bool,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
struct Resourcepack {
    name: String,
    source: String,
    location: String,
    version: String,
    path: Option<PathBuf>,
    #[serde(default = "default_id")]
    id: String,
    authors: Vec<Author>,
    // NEW: Add ignore_update support
    #[serde(default = "default_false")]
    ignore_update: bool,
}

gen_downloadble_impl!(Mod, "mod");
gen_downloadble_impl!(Shaderpack, "shaderpack");
gen_downloadble_impl!(Resourcepack, "resourcepack");
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Loader {
    pub r#type: String,
    pub version: String,
    pub minecraft_version: String,
}

impl Loader {
    pub async fn download(&self, root: &Path, _: &str, http_client: &CachedHttpClient) -> PathBuf {
        match self.r#type.as_str() {
            "fabric" => {
                download_loader_json(
                    &format!(
                        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
                        self.minecraft_version, self.version
                    ),
                    &format!("fabric-loader-{}-{}", self.version, self.minecraft_version),
                    root,
                    http_client,
                )
                .await
            }
            "quilt" => {
                download_loader_json(
                    &format!(
                        "https://meta.quiltmc.org/v3/versions/loader/{}/{}/profile/json",
                        self.minecraft_version, self.version
                    ),
                    &format!("quilt-loader-{}-{}", self.version, self.minecraft_version),
                    root,
                    http_client,
                )
                .await
            }
            _ => panic!("Unsupported loader '{}'!", self.r#type.as_str()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
struct Feature {
    id: String,
    name: String,
    default: bool,
    #[serde(default = "default_hidden")]
    hidden: bool,
    description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
struct RemoteInclude {
    pub location: String,
    pub path: Option<String>,
    #[serde(default = "default_id")]
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub authors: Option<Vec<Author>>,
    // ADD MISSING FIELDS:
    #[serde(default = "default_false")]
    pub optional: bool,
    #[serde(default = "default_false")]
    pub default_enabled: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
struct Manifest {
    manifest_version: i32,
    modpack_version: String,
    name: String,
    subtitle: String,
    tab_group: Option<usize>,
    tab_title: Option<String>,
    tab_color: Option<String>,
    tab_background: Option<String>,
    tab_primary_font: Option<String>,
    tab_secondary_font: Option<String>,
    settings_background: Option<String>,
    popup_title: Option<String>,
    popup_contents: Option<String>,
    description: String,
    icon: bool,
    uuid: String,
    loader: Loader,
    mods: Vec<Mod>,
    shaderpacks: Vec<Shaderpack>,
    resourcepacks: Vec<Resourcepack>,
    remote_include: Option<Vec<RemoteInclude>>,
    include: Vec<Include>,
    features: Vec<Feature>,
    // Add trending indicator field
    trend: Option<bool>,
    #[serde(default = "default_enabled_features")]
    enabled_features: Vec<String>,
    included_files: Option<HashMap<String, Included>>,
    source: Option<String>,
    installer_path: Option<String>,
    max_mem: Option<i32>,
    min_mem: Option<i32>,
    java_args: Option<String>,
    
    // Add the new fields
    category: Option<String>,
    is_new: Option<bool>,
    short_description: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Serialize)]
struct LauncherProfile {
    lastUsed: String,
    lastVersionId: String,
    created: String,
    name: String,
    icon: Option<String>,
    r#type: String,
    gameDir: Option<String>,
    javaDir: Option<String>,
    javaArgs: Option<String>,
    logConfig: Option<String>,
    logConfigIsXML: Option<bool>,
    resolution: Option<HashMap<String, i32>>,
}
#[derive(Debug, Deserialize, Serialize)]
struct ModrinthFile {
    url: String,
    filename: String,
}
#[derive(Debug, Deserialize, Serialize)]
struct ModrinthObject {
    version_number: String,
    files: Vec<ModrinthFile>,
    loaders: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GithubRepo {
    // Theres a lot more fields but we only care about default_branch
    // https://docs.github.com/en/rest/repos/repos?apiVersion=2022-11-28#get-a-repository
    default_branch: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct GithubAsset {
    name: String,
    id: i32,
    browser_download_url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct GithubRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
struct GithubBranch {
    name: String,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Serialize)]
struct MMCComponent {
    #[serde(skip_serializing_if = "Option::is_none")]
    cachedVolatile: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dependencyOnly: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    important: Option<bool>,
    uid: String,
    version: String,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Serialize)]
struct MMCPack {
    components: Vec<MMCComponent>,
    formatVersion: i32,
}

#[derive(Debug)]
enum DownloadError {
    Non200StatusCode(String, u16),
    FailedToParseResponse(String, serde_json::Error),
    IoError(String, std::io::Error),
    HttpError(String, isahc::Error),
    MissingFilename(String),
    CouldNotFindItem(String),
    MedafireMissingDDL(String),
}

impl Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Non200StatusCode(item, x) => write!(
                f,
                "Encountered '{x}' error code when attempting to download: '{item}'"
            ),

            DownloadError::FailedToParseResponse(item, e) => write!(
                f,
                "Failed to parse download response: '{e:#?}' when attempting to download: '{item}'"
            ),
            DownloadError::IoError(item, e) => write!(
                f,
                "Encountered io error: '{e:#?}' when attempting to download: '{item}'"
            ),
            DownloadError::HttpError(item, e) => write!(
                f,
                "Encountered http error: '{e:#?}' when attempting to download: '{item}'"
            ),
            DownloadError::MissingFilename(item) => {
                write!(f, "Could not get filename for: '{item}'")
            }
            DownloadError::CouldNotFindItem(item) => {
                write!(f, "Could not find item: '{item}'")
            }
            DownloadError::MedafireMissingDDL(item) => {
                write!(f, "Could not get DDL link from Nediafire: '{item}'")
            }
        }
    }
}

impl std::error::Error for DownloadError {}
impl DownloadError {
    /// The name/id of the item that failed, for surfacing in error.log / UI
    /// without needing to match on every variant at the call site.
    fn item_name(&self) -> &str {
        match self {
            DownloadError::Non200StatusCode(item, _) => item,
            DownloadError::FailedToParseResponse(item, _) => item,
            DownloadError::IoError(item, _) => item,
            DownloadError::HttpError(item, _) => item,
            DownloadError::MissingFilename(item) => item,
            DownloadError::CouldNotFindItem(item) => item,
            DownloadError::MedafireMissingDDL(item) => item,
        }
    }
}

/// A single item that failed to download during install/update, kept around
/// so we can write error.log and/or show the user a proceed/cancel prompt
/// instead of aborting the whole installation.
#[derive(Debug, Clone)]
pub struct FailedItem {
    pub kind: String,   // "mod" | "shaderpack" | "resourcepack" | "include" | "remote_include"
    pub name: String,
    pub error: String,
}

#[derive(Debug)]
enum LauncherProfileError {
    IoError(std::io::Error),
    InvalidJson(serde_json::Error),
    ProfilesNotObject,
    NoProfiles,
    RootNotObject,
    IconNotFound,
    InvalidIcon(image::error::ImageError),
}

impl Display for LauncherProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LauncherProfileError::IoError(e) => write!(
                f,
                "Encountered IO error when creating launcher profile: {e}"
            ),
            LauncherProfileError::InvalidJson(e) => {
                write!(f, "Invalid 'launcher_profiles.json': {e}")
            }
            LauncherProfileError::NoProfiles => {
                write!(f, "'launcher_profiles.json' missing 'profiles' key")
            }
            LauncherProfileError::ProfilesNotObject => {
                write!(f, "Expected 'launcher_profiles.profiles' to be 'object'")
            }
            LauncherProfileError::RootNotObject => {
                write!(f, "Expected 'launcher_profiles' to be 'object'")
            }
            LauncherProfileError::IconNotFound => {
                write!(f, "'manifest.icon' was set to true but no icon was found")
            }
            LauncherProfileError::InvalidIcon(e) => write!(
                f,
                "Encountered image error when creating launcher profile: {e}"
            ),
        }
    }
}

impl std::error::Error for LauncherProfileError {}

impl From<std::io::Error> for LauncherProfileError {
    fn from(value: std::io::Error) -> Self {
        LauncherProfileError::IoError(value)
    }
}

impl From<serde_json::Error> for LauncherProfileError {
    fn from(value: serde_json::Error) -> Self {
        LauncherProfileError::InvalidJson(value)
    }
}

impl From<image::error::ImageError> for LauncherProfileError {
    fn from(value: image::error::ImageError) -> Self {
        LauncherProfileError::InvalidIcon(value)
    }
}


fn get_filename(headers: &HeaderMap<HeaderValue>, url: &str) -> Result<String, DownloadError> {
    let filename = if let Some(x) = headers.get("content-disposition") {
        let x = x.to_str().unwrap();
        if x.contains("attachment") {
            let re = Regex::new(r#"filename="(.*?)""#).unwrap();
            match match re.captures(x) {
                Some(v) => Ok(v),
                None => Err(DownloadError::MissingFilename(url.to_string())),
            } {
                Ok(v) => v[1].to_string(),
                Err(e) => match url.split('/').next_back() {
                    Some(v) => v.to_string(),
                    None => {
                        return Err(e);
                    }
                }
                .to_string(),
            }
        } else {
            url
                .split('/')
                .next_back()
                .unwrap() // this should be impossible to error because all urls will have "/"s in them and if they dont it gets caught earlier
                .to_string()
        }
    } else {
        url
            .split('/')
            .next_back()
            .unwrap() // this should be impossible to error because all urls will have "/"s in them and if they dont it gets caught earlier
            .to_string()
    };
    Ok(filename)
}

async fn download_loader_json(
    url: &str,
    loader_name: &str,
    root: &Path,
    http_client: &CachedHttpClient,
) -> PathBuf {
    let loader_path = root.join(Path::new(&format!("versions/{}", loader_name)));
    if loader_path
        .join(Path::new(&format!("{}.json", loader_name)))
        .exists()
    {
        return PathBuf::new();
    }
    let resp = http_client
        .get_async(url)
        .await
        .expect("Failed to download loader!")
        .text()
        .await
        .unwrap();
    fs::create_dir_all(&loader_path).expect("Failed to create loader directory");
    fs::write(
        loader_path.join(Path::new(&format!("{}.json", loader_name))),
        resp,
    )
    .expect("Failed to write loader json");
    fs::write(
        loader_path.join(Path::new(&format!("{}.jar", loader_name))),
        "",
    )
    .expect("Failed to write loader dummy jar");
    loader_path
}

async fn download_from_ddl<T: Downloadable + Debug>(
    item: &T,
    modpack_root: &Path,
    r#type: &str,
    http_client: &CachedHttpClient,
) -> Result<PathBuf, DownloadError> {
    let mut resp = match http_client.get_nocache(item.get_location()).await {
        Ok(v) => v,
        Err(e) => return Err(DownloadError::HttpError(item.get_name().to_string(), e)),
    };
    let filename = get_filename(resp.headers(), item.get_location())?;
    let dist = match r#type {
        "mod" => modpack_root.join(Path::new("mods")),
        "resourcepack" => modpack_root.join(Path::new("resourcepacks")),
        "shaderpack" => modpack_root.join(Path::new("shaderpacks")),
        _ => panic!("Unsupported item type: '{}'???", r#type), // this should be impossible
    };
    match fs::create_dir_all(&dist) {
        Ok(_) => (),
        Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
    }
    let final_dist = dist.join(filename);
    debug!("Writing '{}' to '{:#?}'", item.get_name(), final_dist);
    let contents = match resp.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
    };
    match fs::write(&final_dist, contents) {
        Ok(_) => (),
        Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
    };
    Ok(final_dist)
}

async fn download_from_modrinth<T: Downloadable + Debug>(
    item: &T,
    modpack_root: &Path,
    loader_type: &str,
    r#type: &str,
    http_client: &CachedHttpClient,
) -> Result<PathBuf, DownloadError> {
    let mut resp = match http_client
        .get_nocache(format!(
            "https://api.modrinth.com/v2/project/{}/version",
            item.get_location()
        ))
        .await
    {
        Ok(v) => v,
        Err(e) => {
            return Err(DownloadError::HttpError(item.get_name().to_string(), e));
        }
    };
    if resp.status() != StatusCode::OK {
        return Err(DownloadError::Non200StatusCode(
            item.get_name().to_string(),
            resp.status().as_u16(),
        ));
    }
    let resp_text = match resp.text().await {
        Ok(v) => v,
        Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
    };
    let resp_obj: Vec<ModrinthObject> = match serde_json::from_str(&resp_text) {
        Ok(v) => v,
        Err(e) => {
            return Err(DownloadError::FailedToParseResponse(
                item.get_name().to_string(),
                e,
            ));
        }
    };
    let dist = match r#type {
        "mod" => modpack_root.join(Path::new("mods")),
        "resourcepack" => modpack_root.join(Path::new("resourcepacks")),
        "shaderpack" => modpack_root.join(Path::new("shaderpacks")),
        _ => panic!("Unsupported item type: '{}'???", r#type), // this should be impossible
    };
    match fs::create_dir_all(&dist) {
        Ok(_) => (),
        Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
    }
    for _mod in resp_obj {
        if &_mod.version_number == item.get_version()
            && (_mod.loaders.contains(&String::from("minecraft"))
                || _mod.loaders.contains(&String::from(loader_type))
                || r#type == "shaderpack")
        {
            let content = match match http_client.get_nocache(&_mod.files[0].url).await {
                Ok(v) => v,
                Err(e) => return Err(DownloadError::HttpError(item.get_name().to_string(), e)),
            }
            .bytes()
            .await
            {
                Ok(bytes) => bytes,
                Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
            };
            let final_dist = dist.join(Path::new(&_mod.files[0].filename));
            debug!("Writing '{}' to '{:#?}'", item.get_name(), final_dist);
            match fs::write(&final_dist, content) {
                Ok(_) => (),
                Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
            };
            return Ok(final_dist);
        }
    }
    Err(DownloadError::CouldNotFindItem(item.get_name().to_string()))
}

async fn download_from_mediafire<T: Downloadable + Debug>(
    item: &T,
    modpack_root: &Path,
    r#type: &str,
    http_client: &CachedHttpClient,
) -> Result<PathBuf, DownloadError> {
    let mut resp = match http_client.get_nocache(item.get_location()).await {
        Ok(v) => v,
        Err(e) => {
            return Err(DownloadError::HttpError(item.get_name().to_string(), e));
        }
    };
    if resp.status() != StatusCode::OK {
        return Err(DownloadError::Non200StatusCode(
            item.get_name().to_string(),
            resp.status().as_u16(),
        ));
    }
    let mediafire = match resp.text().await {
        Ok(v) => v,
        Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
    };
    let re = Regex::new(r#"Download file"\s*href="(.*?)""#).unwrap(); // wont error pattern is valid
    let ddl = &(match re.captures(&mediafire) {
        Some(v) => v,
        None => {
            return Err(DownloadError::MedafireMissingDDL(
                item.get_name().to_string(),
            ))
        }
    })[1];
    let mut resp = match http_client.get_nocache(ddl).await {
        Ok(v) => v,
        Err(e) => return Err(DownloadError::HttpError(item.get_name().to_string(), e)),
    };
    let cd_header = match std::str::from_utf8(
        match resp.headers().get("content-disposition") {
            Some(v) => v,
            None => return Err(DownloadError::MissingFilename(item.get_name().to_string())),
        }
        .as_bytes(),
    ) {
        Ok(v) => v,
        Err(_) => return Err(DownloadError::MissingFilename(item.get_name().to_string())),
    };
    let filename = if cd_header.contains("attachment") {
        match cd_header.split("filename=").last() {
            Some(v) => v,
            None => return Err(DownloadError::MissingFilename(item.get_name().to_string())),
        }
        .replace('"', "")
    } else {
        return Err(DownloadError::MissingFilename(item.get_name().to_string()));
    };
    let dist = match r#type {
        "mod" => modpack_root.join(Path::new("mods")),
        "resourcepack" => modpack_root.join(Path::new("resourcepacks")),
        "shaderpack" => modpack_root.join(Path::new("shaderpacks")),
        _ => panic!("Unsupported item type'{}'???", r#type), // this should be impossible
    };
    match fs::create_dir_all(&dist) {
        Ok(_) => (),
        Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
    };
    let final_dist = dist.join(filename);
    debug!("Writing '{}' to '{:#?}'", item.get_name(), final_dist);
    let contents = match resp.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
    };
    match fs::write(&final_dist, contents) {
        Ok(_) => (),
        Err(e) => return Err(DownloadError::IoError(item.get_name().to_string(), e)),
    };
    Ok(final_dist)
}

fn get_app_data() -> PathBuf {
    if env::consts::OS == "linux" {
        dirs::home_dir().unwrap()
    } else if env::consts::OS == "windows" || env::consts::OS == "macos" {
        dirs::config_dir().unwrap()
    } else {
        panic!("Unsupported os '{}'!", env::consts::OS)
    }
}

fn get_multimc_folder(multimc: &str) -> Result<PathBuf, String> {
    let path = match env::consts::OS {
        "linux" => {
            // For system packages
            let lpath = get_app_data().join(format!(".local/share/{}", multimc));

            if lpath.exists() {
                lpath
            } else {
                // For Flatpak packages, multimc doesnt have a flatpak so we only need prism
                get_app_data().join(format!(".var/app/org.prismlauncher.PrismLauncher/data/{}", multimc))
            }
        },
        "windows" | "macos" => get_app_data().join(multimc),
        _ => panic!("Unsupported os '{}'!", env::consts::OS),
    };
    match path.metadata() {
        Ok(metadata) => {
            if metadata.is_dir() && path.join("instances").is_dir() {
                Ok(path)
            } else {
                Err(String::from("MultiMC directory is not a valid directory!"))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

fn get_minecraft_folder() -> PathBuf {
    if env::consts::OS == "macos" {
        get_app_data().join("minecraft")
    } else {
        get_app_data().join(".minecraft")
    }
}

fn get_modpack_root(launcher: &Launcher, uuid: &str) -> PathBuf {
    match launcher {
        Launcher::Vanilla(root) => {
            // Use the installations directory structure
            let root = root.join(Path::new(&format!(".WC_OVHL/installations/{}", uuid)));
            fs::create_dir_all(&root).expect("Failed to create modpack folder");
            root
        }
        Launcher::MultiMC(root) => {
            let root = root.join(Path::new(&format!("instances/{}/.minecraft", uuid)));
            fs::create_dir_all(&root).expect("Failed to create modpack folder");
            root
        }
    }
}

fn image_to_base64(img: &DynamicImage) -> String {
    let mut image_data: Vec<u8> = Vec::new();
    img.write_to(&mut Cursor::new(&mut image_data), ImageFormat::Png)
        .unwrap();
    let res_base64 = engine::general_purpose::STANDARD.encode(image_data);
    format!("data:image/png;base64,{}", res_base64)
}

fn create_launcher_profile(
    installer_profile: &InstallerProfile,
    icon_img: Option<DynamicImage>,
) -> Result<(), LauncherProfileError> {
    let now = SystemTime::now();
    let now: DateTime<Utc> = now.into();
    let now = now.to_rfc3339();
    let manifest = &installer_profile.manifest;
    let modpack_root = get_modpack_root(
        installer_profile
            .launcher
            .as_ref()
            .expect("No launcher selected!"),
        &manifest.uuid,
    );
    
    match installer_profile
        .launcher
        .as_ref()
        .expect("Asked to create launcher profile without knowing launcher!")
    {
        Launcher::Vanilla(_) => {
            // Use the downloaded icon if available, otherwise use base64 from assets
            let icon = if let Some(icon_img) = icon_img {
                image_to_base64(&icon_img)
            } else if manifest.icon {
                // Fallback to embedded icon if manifest says we should have one
                let embedded_icon = image::load_from_memory(include_bytes!("assets/icon.png")).unwrap();
                image_to_base64(&embedded_icon)
            } else {
                String::from("Furnace")
            };

            // Build JVM args properly
            let mut jvm_args = String::new();
            
            // Add default optimization flags if no custom args provided
            if manifest.java_args.is_none() {
                jvm_args.push_str("-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M");
            }
            
            if let Some(x) = &manifest.java_args {
                if !jvm_args.is_empty() {
                    jvm_args.push(' ');
                }
                jvm_args.push_str(x);
            }
            
            if let Some(x) = manifest.max_mem {
                if !jvm_args.is_empty() {
                    jvm_args.push(' ');
                }
                jvm_args.push_str(&format!("-Xmx{}M", x));
            }
            
            if let Some(x) = manifest.min_mem {
                if !jvm_args.is_empty() {
                    jvm_args.push(' ');
                }
                jvm_args.push_str(&format!("-Xms{}M", x));
            }

            let profile = LauncherProfile {
                lastUsed: now.to_string(),
                lastVersionId: match &manifest.loader.r#type[..] {
                    "fabric" => format!(
                        "fabric-loader-{}-{}",
                        manifest.loader.version, manifest.loader.minecraft_version
                    ),
                    "quilt" => format!(
                        "quilt-loader-{}-{}",
                        manifest.loader.version, manifest.loader.minecraft_version
                    ),
                    _ => panic!("Invalid loader"),
                },
                created: now,
                name: manifest.name.clone(), // Use the installation name, not subtitle
                icon: Some(icon),
                r#type: String::from("custom"),
                gameDir: Some(modpack_root.to_str().unwrap().to_string()),
                javaDir: None,
                javaArgs: if jvm_args.is_empty() {
                    None
                } else {
                    Some(jvm_args)
                },
                logConfig: None,
                logConfigIsXML: None,
                resolution: None,
            };

            let lp_file_path = get_minecraft_folder().join(Path::new("launcher_profiles.json"));

            fs::create_dir_all(get_minecraft_folder())?;
            
            // Ensure launcher_profiles.json exists
            if !lp_file_path.exists() {
                // Create default launcher_profiles.json
                let default_profiles = serde_json::json!({
                    "profiles": {},
                    "settings": {
                        "enableAdvanced": false,
                        "enableAnalytics": true,
                        "enableHistorical": false,
                        "enableReleases": true,
                        "enableSnapshots": false,
                        "keepLauncherOpen": false,
                        "profileSorting": "byName",
                        "showGameLog": false,
                        "showMenu": false,
                        "soundOn": false
                    },
                    "version": 3
                });
                
                fs::write(&lp_file_path, serde_json::to_string_pretty(&default_profiles)?)?;
                debug!("Created default launcher_profiles.json");
            }

            let mut lp_obj: JsonValue = serde_json::from_str(&fs::read_to_string(&lp_file_path)?)?;
            
            // Ensure profiles object exists
            if !lp_obj.is_object() {
                return Err(LauncherProfileError::RootNotObject);
            }
            
            let obj = lp_obj.as_object_mut().unwrap();
            if !obj.contains_key("profiles") {
                obj.insert("profiles".to_string(), serde_json::Value::Object(serde_json::Map::new()));
            }
            
            let profiles = obj.get_mut("profiles").unwrap().as_object_mut()
                .ok_or(LauncherProfileError::ProfilesNotObject)?;
            
            // Insert or update the profile using the UUID as the key
            profiles.insert(manifest.uuid.clone(), serde_json::to_value(profile)?);
            
            fs::write(lp_file_path, serde_json::to_string_pretty(&lp_obj)?)?;
            debug!("Successfully created/updated launcher profile for: {} (UUID: {})", manifest.name, manifest.uuid);
        }
        Launcher::MultiMC(root) => {
            // MultiMC/Prism Launcher profile creation
            let instance_path = root.join("instances").join(&manifest.uuid);
            fs::create_dir_all(&instance_path)?;

            // Save icon into the launcher's shared icons/ folder (not the instance
            // folder) and reference it via iconKey, matching how MultiMC/Prism
            // actually resolve custom instance icons.
            let icons_dir = root.join("icons");
            let _ = fs::create_dir_all(&icons_dir);
            let icon_key = if let Some(icon_img) = icon_img {
                let icon_path = icons_dir.join(format!("{}.png", manifest.uuid));
                match icon_img.save(&icon_path) {
                    Ok(_) => {
                        debug!("Saved instance icon to: {:?}", icon_path);
                        manifest.uuid.clone()
                    }
                    Err(e) => {
                        debug!("Failed to save instance icon: {}", e);
                        String::from("default")
                    }
                }
            } else if manifest.icon {
                let embedded_icon = image::load_from_memory(include_bytes!("assets/icon.png")).unwrap();
                let icon_path = icons_dir.join(format!("{}.png", manifest.uuid));
                match embedded_icon.save(&icon_path) {
                    Ok(_) => {
                        debug!("Saved embedded icon to: {:?}", icon_path);
                        manifest.uuid.clone()
                    }
                    Err(e) => {
                        debug!("Failed to save embedded icon: {}", e);
                        String::from("default")
                    }
                }
            } else {
                String::from("default")
            };

            let instance_cfg_path = instance_path.join("instance.cfg");

            // Only write instance.cfg if it doesn't already exist, so we don't
            // clobber settings the user has changed inside MultiMC/Prism itself
            // on every subsequent update/modify.
            if !instance_cfg_path.exists() {
                // NOTE: the correct MultiMC/Prism override keys are
                // MaxMemAlloc / MinMemAlloc, NOT MaxMemory / MinMemory.
                // Using the wrong keys means OverrideMemory=true is set but
                // silently ignored, and the instance falls back to whatever
                // the launcher's global default happens to be.
                let max_mem = manifest
                    .max_mem
                    .map(|v| format!("\nMaxMemAlloc={}", v))
                    .unwrap_or_default();
                let min_mem = manifest
                    .min_mem
                    .map(|v| format!("\nMinMemAlloc={}", v))
                    .unwrap_or_default();
                let override_mem = if max_mem.is_empty() && min_mem.is_empty() {
                    ""
                } else {
                    "\nOverrideMemory=true"
                };

                let jvm_args = manifest
                    .java_args
                    .as_ref()
                    .map(|v| format!("\nJvmArgs={}\nOverrideJavaArgs=true", v))
                    .unwrap_or_default();

                let instance_cfg = format!(
                    "InstanceType=OneSix\niconKey={}\nname={}{}{}{}{}\n",
                    icon_key, manifest.name, max_mem, min_mem, override_mem, jvm_args
                );

                fs::write(&instance_cfg_path, instance_cfg)?;
            } else {
                debug!("instance.cfg already exists for {}, leaving user settings intact", manifest.uuid);
            }
            
            // Create mmc-pack.json for MultiMC/Prism.
            // Deliberately omit cachedVolatile: that field is metadata the
            // launcher writes itself once it has resolved a component against
            // its online meta index. Setting it ourselves without the backing
            // cache data can make the launcher think the pack needs
            // re-resolving before it can launch.
            let mmc_pack = MMCPack {
                components: vec![
                    MMCComponent {
                        cachedVolatile: None,
                        dependencyOnly: None,
                        important: Some(true),
                        uid: String::from("net.minecraft"),
                        version: manifest.loader.minecraft_version.clone(),
                    },
                    MMCComponent {
                        cachedVolatile: None,
                        dependencyOnly: None,
                        important: None,
                        uid: match &manifest.loader.r#type[..] {
                            "fabric" => String::from("net.fabricmc.fabric-loader"),
                            "quilt" => String::from("org.quiltmc.quilt-loader"),
                            _ => panic!("Invalid loader"),
                        },
                        version: manifest.loader.version.clone(),
                    },
                ],
                formatVersion: 1,
            };
            
            // mmc-pack.json IS always rewritten (unlike instance.cfg above) —
            // this is what carries Minecraft/loader version bumps on update,
            // so it needs to reflect the current manifest every time.
            fs::write(
                instance_path.join("mmc-pack.json"),
                serde_json::to_string_pretty(&mmc_pack)?,
            )?;

            debug!("Successfully created MultiMC/Prism instance: {} (UUID: {})", manifest.name, manifest.uuid);
        }
    }
    Ok(())
}

// Add function to delete launcher profile
pub fn delete_launcher_profile(installation_uuid: &str, launcher_type: &str) -> Result<(), String> {
    debug!("Deleting launcher profile for installation: {}", installation_uuid);
    
    match launcher_type {
        "vanilla" => {
            let lp_file_path = get_minecraft_folder().join("launcher_profiles.json");
            
            if lp_file_path.exists() {
                let content = fs::read_to_string(&lp_file_path)
                    .map_err(|e| format!("Failed to read launcher profiles: {}", e))?;
                    
                let mut lp_obj: JsonValue = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse launcher profiles: {}", e))?;
                
                if let Some(profiles) = lp_obj.get_mut("profiles").and_then(|p| p.as_object_mut()) {
                    if profiles.remove(installation_uuid).is_some() {
                        fs::write(&lp_file_path, serde_json::to_string_pretty(&lp_obj)
                            .map_err(|e| format!("Failed to serialize profiles: {}", e))?)
                            .map_err(|e| format!("Failed to write launcher profiles: {}", e))?;
                        
                        debug!("Successfully removed profile from vanilla launcher");
                    } else {
                        debug!("Profile {} not found in vanilla launcher", installation_uuid);
                    }
                }
            }
        },
        launcher_type if launcher_type.starts_with("multimc") || launcher_type.starts_with("custom") => {
            // For MultiMC/Prism, the instance directory is already deleted by the installation deletion
            debug!("MultiMC/Prism instance will be deleted with installation directory");
        },
        _ => {
            debug!("Unknown launcher type for profile deletion: {}", launcher_type);
        }
    }
    
    Ok(())
}

/// Panics:
///     If path is not located in modpack_root
macro_rules! validate_item_path {
    ($item:expr, $modpack_root:expr) => {
        if $item.get_path().is_some() {
            if $item
                .get_path()
                .as_ref()
                .unwrap()
                .parent()
                .expect("Illegal item file path!")
                .parent()
                .expect("Illegal item dir path!")
                == $modpack_root
            {
                $item
            } else {
                panic!("{:?}'s path was not located in modpack root!", $item);
            }
        } else {
            $item
        }
    };
}

fn get_installed_packs(launcher: &Launcher) -> Result<Vec<PackName>, std::io::Error> {
    let mut packs = vec![];
    let manifest_paths: Vec<PathBuf> = match launcher {
        Launcher::Vanilla(root) => {
            fs::read_dir(root.join(".WC_OVHL/"))?.filter_map(|entry| {
                let path = entry.ok()?.path().join("manifest.json");
                if path.exists() {Some(path)} else {None}
            }).collect()
        },
        Launcher::MultiMC(root) => {
            fs::read_dir(root.join("instances/"))?.filter_map(|entry| {
                let path = entry.ok()?.path().join(".minecraft/manifest.json");
                if path.exists() {Some(path)} else {None}
            }).collect()
        },
    };
    for path in manifest_paths {
        let manifest: Result<Manifest, serde_json::Error> = serde_json::from_str(&fs::read_to_string(path).unwrap());
        if let Ok(manifest) = manifest {
            packs.push(PackName { name: manifest.subtitle, uuid: manifest.uuid })
        }
    }
    
    Ok(packs)
}

fn uninstall(launcher: &Launcher, uuid: &str) -> Result<(), std::io::Error> {
    info!("Uninstalling modpack: '{uuid}'!");
    let instance = match launcher {
        Launcher::Vanilla(root) => {
            root.join(format!(".WC_OVHL/{uuid}"))
        }
        Launcher::MultiMC(root) => {
            root.join(format!("instances/{uuid}/.minecraft"))
        }
    };
    if instance.is_dir() {
        fs::remove_dir_all(&instance)?;
        info!("Removed: {instance:#?}");
        fs::create_dir(instance)?;
    } else {
        error!("Failed to uninstall '{uuid}'");
    }
    let _ = isahc::post(
        "https://tracking.commander07.workers.dev/track",
        format!(
            "{{
        \"projectId\": \"55db8403a4f24f3aa5afd33fd1962888\",
        \"dataSourceId\": \"{uuid}\",
        \"userAction\": \"uninstall\",
        \"additionalData\": {{}}
    }}"));
    info!("Uninstalled modpack!");
    Ok(())
}

async fn download_helper<T: Downloadable + Debug, F: FnMut() + Clone>(
    items: Vec<T>,
    enabled_features: &Vec<String>,
    modpack_root: &Path,
    loader_type: &str,
    http_client: &CachedHttpClient,
    progress_callback: F,
    is_update: bool,
    ignore_update_items: &std::collections::HashSet<String>,
    item_kind: &str,
) -> Result<(Vec<T>, Vec<FailedItem>), DownloadError> {
    let results = futures::stream::iter(items.into_iter().map(|item| async {
        // FIXED: Proper logic for determining if item should be included
        let should_include = if item.get_id() == "default" {
            // Always include the "default" item
            debug!("Including default item: {}", item.get_name());
            true
        } else {
            // Check if this item should be included based on enabled_features
            let is_enabled = enabled_features.contains(item.get_id());
            debug!("Item '{}' (ID: {}) - enabled: {}, in features: {:?}", 
                   item.get_name(), item.get_id(), is_enabled, enabled_features);
            is_enabled
        };
        
        // Check if we should ignore this item during updates
        let should_ignore_update = is_update && ignore_update_items.contains(item.get_id());
        
        if item.get_path().is_none() && should_include && !should_ignore_update {
            debug!("Downloading item: {} (ID: {})", item.get_name(), item.get_id());
            let download_result = item
                .download(modpack_root, loader_type, http_client)
                .await;
            (progress_callback.clone())();
            let path = download_result?;
            Ok(T::new(
                item.get_name().to_owned(),
                item.get_source().to_owned(),
                item.get_location().to_owned(),
                item.get_version().to_owned(),
                Some(path),
                item.get_id().to_owned(),
                item.get_authors().to_owned(),
            ))
        } else {
            let item = validate_item_path!(item, modpack_root);
            
            
            let path = if should_ignore_update && item.get_path().is_some() {
                debug!("Ignoring update for: '{}' (ignore_update=true)", item.get_name());
                item.get_path().to_owned()
            } else if !should_include && item.get_path().is_some() {
                debug!("Removing disabled item: '{}' (not in enabled_features)", item.get_name());
                let _ = fs::remove_file(item.get_path().as_ref().unwrap());
                None
            } else if !should_include {
                debug!("Skipping disabled item: '{}' (not in enabled_features)", item.get_name());
                None
            } else {
                debug!("Keeping existing item: '{}' (enabled)", item.get_name());
                item.get_path().to_owned()
            };
            
            Ok(T::new(
                item.get_name().to_owned(),
                item.get_source().to_owned(),
                item.get_location().to_owned(),
                item.get_version().to_owned(),
                path,
                item.get_id().to_owned(),
                item.get_authors().to_owned(),
            ))
        }
    }))
    .buffer_unordered(CONCURRENCY)
    .collect::<Vec<Result<T, DownloadError>>>()
    .await;
    
    let mut return_vec = vec![];
    let mut failures = vec![];
    for res in results {
        match res {
            Ok(v) => return_vec.push(v),
            Err(e) => {
                warn!(
                    "Skipping {} '{}' after download failure: {}",
                    item_kind,
                    e.item_name(),
                    e
                );
                failures.push(FailedItem {
                    kind: item_kind.to_string(),
                    name: e.item_name().to_string(),
                    error: e.to_string(),
                });
            }
        }
    }
    Ok((return_vec, failures))
}

async fn download_zip(name: &str, http_client: &CachedHttpClient, url: &str, path: &Path) -> Result<Vec<String>, DownloadError> {
    debug!("Downloading '{}'", name);
    let mut files: Vec<String> = vec![];
    // download and unzip in modpack root
    let mut tries = 0;
    let mut content_resp = match loop {
        let content_resp = http_client
            .with_headers(
                url,
                &[("Accept", "application/octet-stream")],
            )
            .await;
        if content_resp.is_err() {
            tries += 1;
            if tries >= ATTEMPTS {
                break Err(content_resp.err().unwrap());
            }
        } else {
            break Ok(content_resp.unwrap());
        }
    } {
        Ok(v) => v,
        Err(e) => return Err(DownloadError::HttpError(name.to_string(), e)),
    };
    let content_byte_resp = match content_resp.bytes().await {
        Ok(v) => v,
        Err(e) => return Err(DownloadError::IoError(name.to_string(), e)),
    };
    fs::create_dir_all(path).expect("Failed to create unzip path");
    let zipfile_path = path.join("tmp_include.zip");
    fs::write(&zipfile_path, content_byte_resp)
        .expect("Failed to write 'tmp_include.zip'!");
    debug!("Downloaded '{}'", name);
    debug!("Unzipping '{}'", name);
    let zipfile = fs::File::open(&zipfile_path).unwrap();
    let mut archive = zip::ZipArchive::new(zipfile).unwrap();
    // modified from https://github.com/zip-rs/zip/blob/e32db515a2a4c7d04b0bf5851912a399a4cbff68/examples/extract.rs#L19
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let outpath = match file.enclosed_name() {
            Some(outpath) => path.join(outpath),
            None => continue,
        };
        if (*file.name()).ends_with('/') {
            fs::create_dir_all(&outpath).unwrap();
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).unwrap();
                }
            }
            let mut outfile = fs::File::create(&outpath).unwrap();
            std::io::copy(&mut file, &mut outfile).unwrap();
            files.push(outpath.to_str().unwrap().to_string());
        }
    }
    fs::remove_file(&zipfile_path).expect("Failed to remove tmp 'tmp_include.zip'!");
    debug!("Unzipped '{}'", name);
    Ok(files)
}

// In src/main.rs - Add this helper function before the install function

fn resolve_dependencies(
    feature_id: &str,
    enabled_features: &mut Vec<String>,
    universal_manifest: &UniversalManifest,
) {
    // Check all component types for dependencies
    let all_components: Vec<(&str, Option<&Vec<String>>)> = universal_manifest.mods.iter()
        .map(|c| (c.id.as_str(), c.dependencies.as_ref()))
        .chain(universal_manifest.shaderpacks.iter()
            .map(|c| (c.id.as_str(), c.dependencies.as_ref())))
        .chain(universal_manifest.resourcepacks.iter()
            .map(|c| (c.id.as_str(), c.dependencies.as_ref())))
        .chain(universal_manifest.include.iter()
            .map(|c| (c.id.as_str(), c.dependencies.as_ref())))
        .chain(universal_manifest.remote_include.iter()
            .map(|c| (c.id.as_str(), c.dependencies.as_ref())))
        .collect();
    
    // Find the component
    if let Some((_, Some(deps))) = all_components.iter().find(|(id, _)| id == &feature_id) {
        for dep in deps.iter() {
            if !enabled_features.contains(dep) {
                debug!("Auto-enabling dependency {} for {}", dep, feature_id);
                enabled_features.push(dep.clone());
                // Recursively resolve dependencies of dependencies
                resolve_dependencies(dep, enabled_features, universal_manifest);
            }
        }
    }
}

// In src/main.rs - Update the install function's feature resolution section

async fn install<F: FnMut() + Clone>(installer_profile: &InstallerProfile, progress_callback: F) -> Result<Vec<FailedItem>, String> {
    info!("Installing modpack");
    
    // Get the universal manifest to properly determine what should be installed
    let universal_manifest = match crate::universal::load_universal_manifest(&installer_profile.http_client, None).await {
        Ok(manifest) => manifest,
        Err(e) => {
            error!("Failed to load universal manifest: {:?}", e);
            return Err(format!("Failed to load universal manifest: {:?}", e));
        }
    };
    
    // Build the complete list of features that should be enabled
    let mut effective_enabled_features = installer_profile.enabled_features.clone();
    
    // Add all default-enabled components (keeping existing logic)
    for component in &universal_manifest.mods {
        if component.default_enabled && !effective_enabled_features.contains(&component.id) {
            debug!("Adding default-enabled mod: {} ({})", component.id, component.name);
            effective_enabled_features.push(component.id.clone());
        }
    }
    
    for component in &universal_manifest.shaderpacks {
        if component.default_enabled && !effective_enabled_features.contains(&component.id) {
            debug!("Adding default-enabled shaderpack: {} ({})", component.id, component.name);
            effective_enabled_features.push(component.id.clone());
        }
    }
    
    for component in &universal_manifest.resourcepacks {
        if component.default_enabled && !effective_enabled_features.contains(&component.id) {
            debug!("Adding default-enabled resourcepack: {} ({})", component.id, component.name);
            effective_enabled_features.push(component.id.clone());
        }
    }
    
    for include in &universal_manifest.include {
        if include.default_enabled && !include.id.is_empty() && !effective_enabled_features.contains(&include.id) {
            debug!("Adding default-enabled include: {} ({})", include.id, include.location);
            effective_enabled_features.push(include.id.clone());
        }
    }
    
    for remote in &universal_manifest.remote_include {
        if remote.default_enabled && !effective_enabled_features.contains(&remote.id) {
            debug!("Adding default-enabled remote include: {} ({})", 
                   remote.id, remote.name.as_ref().unwrap_or(&remote.id));
            effective_enabled_features.push(remote.id.clone());
        }
    }
    
    // Always ensure "default" is in the list
    if !effective_enabled_features.contains(&"default".to_string()) {
        effective_enabled_features.insert(0, "default".to_string());
    }
    
    // Resolve dependencies for all enabled features
    let features_to_check = effective_enabled_features.clone();
    for feature in features_to_check {
        resolve_dependencies(&feature, &mut effective_enabled_features, &universal_manifest);
    }
    
    // Remove duplicates while preserving order
    let mut seen = std::collections::HashSet::new();
    effective_enabled_features.retain(|item| seen.insert(item.clone()));
    
    debug!("Final enabled features list: {:?}", effective_enabled_features);
    
    // UPDATED: Calculate weighted progress points based on expected time/complexity
    let mut total_progress_points = 0;
    let mut download_counts = (0, 0, 0, 0, 0); // (mods, shaders, resources, includes, remote_includes)
    
    let is_update = installer_profile.installed;
    
    // Count what will be downloaded
    for mod_item in &installer_profile.manifest.mods {
        let should_include = mod_item.id == "default" || effective_enabled_features.contains(&mod_item.id);
        let needs_download = should_include && mod_item.path.is_none();
        if needs_download {
            download_counts.0 += 1;
        }
    }
    
    for shader in &installer_profile.manifest.shaderpacks {
        let should_include = shader.id == "default" || effective_enabled_features.contains(&shader.id);
        let needs_download = should_include && shader.path.is_none();
        if needs_download {
            download_counts.1 += 1;
        }
    }
    
    for resource in &installer_profile.manifest.resourcepacks {
        let should_include = resource.id == "default" || effective_enabled_features.contains(&resource.id);
        let needs_download = should_include && resource.path.is_none();
        if needs_download {
            download_counts.2 += 1;
        }
    }
    
    for include in &installer_profile.manifest.include {
        let should_include = if include.id.is_empty() || include.id == "default" {
            true
        } else if !include.optional {
            true
        } else {
            effective_enabled_features.contains(&include.id)
        };
        
        if should_include {
            download_counts.3 += 1;
        }
    }
    
    if let Some(remote_includes) = &installer_profile.manifest.remote_include {
        for remote in remote_includes {
            let should_include = if remote.id == "default" {
                true
            } else if !remote.optional {
                true
            } else {
                effective_enabled_features.contains(&remote.id)
            };
            
            if should_include {
                download_counts.4 += 1;
            }
        }
    }
    
    // WEIGHT CALCULATION: Assign points based on typical download time/complexity
    // Fast downloads (mods/shaders/resources): 1 point each
    // Medium downloads (includes): 5 points each  
    // Slow downloads (remote includes): 15 points each
    // Overhead tasks: 2 points each
    
    let mod_points = download_counts.0;
    let shader_points = download_counts.1;
    let resource_points = download_counts.2;
    let include_points = download_counts.3 * 5;
    let remote_include_points = download_counts.4 * 15;
    let overhead_points = 4 * 2; // 4 overhead tasks * 2 points each
    
    total_progress_points = mod_points + shader_points + resource_points + include_points + remote_include_points + overhead_points;
    
    debug!("Progress weighting: Mods({}*1={}), Shaders({}*1={}), Resources({}*1={}), Includes({}*5={}), Remote({}*15={}), Overhead(4*2=8), Total: {}", 
           download_counts.0, mod_points,
           download_counts.1, shader_points, 
           download_counts.2, resource_points,
           download_counts.3, include_points,
           download_counts.4, remote_include_points,
           total_progress_points);
    
    let modpack_root = &get_modpack_root(
        installer_profile
            .launcher
            .as_ref()
            .expect("Launcher not selected!"),
        &installer_profile.manifest.uuid,
    );
    let manifest = &installer_profile.manifest;
    let http_client = &installer_profile.http_client;
    let minecraft_folder = get_minecraft_folder();
    
    // Collect items that should be ignored during updates
    let mut ignore_update_items = std::collections::HashSet::new();
    
    for mod_component in &universal_manifest.mods {
        if mod_component.ignore_update {
            ignore_update_items.insert(mod_component.id.clone());
        }
    }
    for shader in &universal_manifest.shaderpacks {
        if shader.ignore_update {
            ignore_update_items.insert(shader.id.clone());
        }
    }
    for resource in &universal_manifest.resourcepacks {
        if resource.ignore_update {
            ignore_update_items.insert(resource.id.clone());
        }
    }
    for include in &universal_manifest.include {
        if include.ignore_update {
            ignore_update_items.insert(include.id.clone());
        }
    }
    for remote in &universal_manifest.remote_include {
        if remote.ignore_update {
            ignore_update_items.insert(remote.id.clone());
        }
    }
    
    let loader_future = match installer_profile.launcher.as_ref().unwrap() {
        Launcher::Vanilla(_) => Some(manifest.loader.download(
            &minecraft_folder,
            &manifest.loader.r#type,
            http_client,
        )),
        Launcher::MultiMC(_) => None,
    };
    
    // UPDATED: Create weighted progress tracker
    let current_progress = std::sync::Arc::new(std::sync::Mutex::new(0));
    
    // Create different progress callbacks for different operation types
    let create_weighted_callback = |weight: i32| {
        let current_progress = current_progress.clone();
        let mut main_callback = progress_callback.clone();
        let total = total_progress_points;
        
        move || {
            if let Ok(mut progress) = current_progress.lock() {
                *progress += weight;
                let current = *progress;
                let percentage = if total > 0 { (current * 100) / total } else { 0 };
                debug!("Weighted progress: +{} points, now {}/{} ({}%)", weight, current, total, percentage);
            }
            main_callback();
        }
    };
    
    // Different callbacks for different operation types
    let mod_callback = create_weighted_callback(1);      // 1 point per mod
    let shader_callback = create_weighted_callback(1);   // 1 point per shader
    let resource_callback = create_weighted_callback(1); // 1 point per resource
    let mut include_callback = create_weighted_callback(5);  // 5 points per include
    let mut remote_callback = create_weighted_callback(15);  // 15 points per remote include
    let mut overhead_callback = create_weighted_callback(2); // 2 points per overhead task
    
    debug!("Starting downloads with weighted progress...");
    
    // Collects every item that failed to download across all categories, so a
    // single dead link doesn't abort the whole installation. Written out to
    // error.log at the end.
    let mut all_failures: Vec<FailedItem> = Vec::new();
    
    // Download components with appropriate weight callbacks
    let (mods_w_path, mod_failures) = match download_helper(
        manifest.mods.clone(),
        &effective_enabled_features,
        modpack_root.as_path(),
        &manifest.loader.r#type,
        http_client,
        mod_callback,
        is_update,
        &ignore_update_items,
        "mod",
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return Err(e.to_string()),
    };
    all_failures.extend(mod_failures);
    
    let (shaderpacks_w_path, shader_failures) = match download_helper(
        manifest.shaderpacks.clone(),
        &effective_enabled_features,
        modpack_root.as_path(),
        &manifest.loader.r#type,
        http_client,
        shader_callback,
        is_update,
        &ignore_update_items,
        "shaderpack",
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return Err(e.to_string()),
    };
    all_failures.extend(shader_failures);
    
    let (resourcepacks_w_path, resource_failures) = match download_helper(
        manifest.resourcepacks.clone(),
        &effective_enabled_features,
        modpack_root.as_path(),
        &manifest.loader.r#type,
        http_client,
        resource_callback,
        is_update,
        &ignore_update_items,
        "resourcepack",
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return Err(e.to_string()),
    };
    all_failures.extend(resource_failures);
    
    let mut included_files: HashMap<String, crate::Included> = HashMap::new();
    
    // Handle regular includes with weighted progress
    if !manifest.include.is_empty() {
        debug!("Processing {} includes from manifest", manifest.include.len());
        
        for inc in &manifest.include {
            if is_update && ignore_update_items.contains(&inc.id) {
                debug!("Ignoring update for include: {} (ignore_update=true)", inc.id);
                continue;
            }
            
            let should_install = if inc.id.is_empty() || inc.id == "default" {
                true
            } else if !inc.optional {
                true
            } else {
                effective_enabled_features.contains(&inc.id)
            };
            
            if !should_install {
                debug!("Skipping disabled include: {} (not in effective features)", inc.id);
                continue;
            }
        
            debug!("Processing include: {} (weight: 5 points)", inc.id);
            
            let github_url = format!(
                "https://raw.githubusercontent.com/Wynncraft-Overhaul/majestic-overhaul/master/{}",
                inc.location
            );
            
            let target_path = validate_safe_path(modpack_root, &inc.location)
                .map_err(|e| format!("Security error for include {}: {}", inc.location, e))?;
            
            let is_file = inc.location.ends_with(".zip") || 
                         inc.location.ends_with(".txt") || 
                         inc.location == "options.txt" ||
                         (inc.location.contains('.') && !inc.location.starts_with("."));
            
            let is_directory = inc.location == "config" || 
                              inc.location.starts_with(".") ||
                              (!inc.location.contains('.') && !is_file);
            
            if is_file {
                if let Some(parent) = target_path.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        error!("Failed to create directory for include {}: {}", inc.location, e);
                        continue;
                    }
                }
                
                match http_client.get_async(&github_url).await {
                    Ok(mut response) => {
                        if response.status() == StatusCode::OK {
                            match response.bytes().await {
                                Ok(bytes) => {
                                    match fs::write(&target_path, bytes) {
                                        Ok(_) => {
                                            debug!("Successfully downloaded include file: {}", inc.location);
                                            included_files.insert(
                                                inc.id.clone(),
                                                crate::Included {
                                                    md5: String::new(),
                                                    files: vec![target_path.to_string_lossy().to_string()],
                                                }
                                            );
                                            include_callback(); // +5 points
                                        },
                                        Err(e) => {
                                            error!("Failed to write include file {}: {}", inc.location, e);
                                            all_failures.push(FailedItem {
                                                kind: "include".to_string(),
                                                name: inc.location.clone(),
                                                error: format!("Failed to write file: {}", e),
                                            });
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("Failed to read include file bytes: {}", e);
                                    all_failures.push(FailedItem {
                                        kind: "include".to_string(),
                                        name: inc.location.clone(),
                                        error: format!("Failed to read response: {}", e),
                                    });
                                }
                            }
                        } else {
                            error!("Failed to download include {}: HTTP {}", inc.location, response.status());
                            all_failures.push(FailedItem {
                                kind: "include".to_string(),
                                name: inc.location.clone(),
                                error: format!("HTTP {}", response.status()),
                            });
                        }
                    },
                    Err(e) => {
                        error!("Failed to download include {}: {}", inc.location, e);
                        all_failures.push(FailedItem {
                            kind: "include".to_string(),
                            name: inc.location.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            } else if is_directory {
                if let Err(e) = fs::create_dir_all(&target_path) {
                    error!("Failed to create directory {}: {}", target_path.display(), e);
                    all_failures.push(FailedItem {
                        kind: "include".to_string(),
                        name: inc.location.clone(),
                        error: format!("Failed to create directory: {}", e),
                    });
                    continue;
                }
                
                let api_url = format!(
                    "https://api.github.com/repos/Wynncraft-Overhaul/majestic-overhaul/contents/{}",
                    inc.location
                );
                
                match download_github_directory(http_client, &api_url, &inc.location, modpack_root).await {
                    Ok(files) => {
                        debug!("Successfully downloaded include directory: {} ({} files)", inc.location, files.len());
                        included_files.insert(
                            inc.id.clone(),
                            crate::Included {
                                md5: String::new(),
                                files,
                            }
                        );
                        include_callback(); // +5 points
                    },
                    Err(e) => {
                        error!("Failed to download include directory {}: {}", inc.location, e);
                        all_failures.push(FailedItem {
                            kind: "include".to_string(),
                            name: inc.location.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }
        }
    }
    
    // Handle remote includes with weighted progress (highest weight!)
    // Handle remote includes with weighted progress (highest weight!)
    if let Some(remote_includes) = &manifest.remote_include {
        debug!("Processing {} remote includes from manifest", remote_includes.len());
        
        for remote in remote_includes {
            if is_update && ignore_update_items.contains(&remote.id) {
                debug!("Ignoring update for remote include: {} (ignore_update=true)", remote.id);
                continue;
            }
            
            let should_install = if remote.id == "default" {
                true
            } else if !remote.optional {
                true
            } else {
                effective_enabled_features.contains(&remote.id)
            };
            
            if !should_install {
                debug!("Skipping disabled remote include: {} (not in effective features)", remote.id);
                continue;
            }
            
            let name = remote.name.clone().unwrap_or_else(|| remote.id.clone());
            debug!("Processing remote include: {} (weight: 15 points)", name);
            
            let target_path = if let Some(path) = &remote.path {
                modpack_root.join(path)
            } else {
                modpack_root.clone()
            };
            
            match download_zip(&name, http_client, &remote.location, &target_path).await {
                Ok(files) => {
                    debug!("Successfully downloaded remote include: {} ({} files)", name, files.len());
                    included_files.insert(
                        remote.id.clone(),
                        crate::Included {
                            md5: remote.version.clone(),
                            files,
                        }
                    );
                    remote_callback(); // +15 points - BIG progress jump here!
                },
                Err(e) => {
                    error!("Failed to download remote include {}: {:?}", name, e);
                    all_failures.push(FailedItem {
                        kind: "remote_include".to_string(),
                        name: name.clone(),
                        error: format!("{:?}", e),
                    });
                    // Still count the progress weight for this item so the
                    // bar doesn't stall waiting on a file that will never load.
                    remote_callback();
                }
            }
        }
    }

    // Handle overhead tasks with weighted progress
    debug!("Starting overhead tasks (2 points each)");

    // Save local manifest
    let local_manifest = crate::Manifest {
        mods: mods_w_path,
        shaderpacks: shaderpacks_w_path,
        resourcepacks: resourcepacks_w_path,
        enabled_features: effective_enabled_features.clone(),
        included_files: Some(included_files),
        source: Some(format!(
            "{}{}",
            installer_profile.modpack_source, installer_profile.modpack_branch
        )),
        installer_path: Some(
            env::current_exe()
                .unwrap()
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
                .replace("\\\\?\\", ""),
        ),
        ..manifest.clone()
    };

    fs::write(
        modpack_root.join(Path::new("manifest.json")),
        serde_json::to_string(&local_manifest).expect("Failed to parse 'manifest.json'!"),
    )
    .expect("Failed to save a local copy of 'manifest.json'!");

    overhead_callback(); // +2 points

    // Download icon if needed
    let icon_img = if manifest.icon {
        let icon_url = "https://raw.githubusercontent.com/Wynncraft-Overhaul/installer/master/src/assets/icon.png";
        match http_client.get_async(icon_url).await {
            Ok(mut resp) => {
                match resp.bytes().await {
                    Ok(bytes) => {
                        match image::ImageReader::new(std::io::Cursor::new(bytes))
                            .with_guessed_format() {
                            Ok(reader) => {
                                match reader.decode() {
                                    Ok(img) => Some(img),
                                    Err(e) => {
                                        error!("Failed to decode icon: {}", e);
                                        None
                                    }
                                }
                            },
                            Err(e) => {
                                error!("Failed to guess icon format: {}", e);
                                None
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to read icon bytes: {}", e);
                        None
                    }
                }
            },
            Err(e) => {
                error!("Failed to download icon: {}", e);
                None
            }
        }
    } else {
        None
    };

    overhead_callback(); // +2 points

    match create_launcher_profile(installer_profile, icon_img) {
        Ok(_) => {
            debug!("Launcher profile created successfully");
        },
        Err(e) => return Err(e.to_string()),
    };

    overhead_callback(); // +2 points

    if loader_future.is_some() {
        loader_future.unwrap().await;
    }

    overhead_callback(); // +2 points - FINAL

    debug!("All installation tasks completed");

    // Update installation state
    if let Ok(mut installation) = crate::installation::load_installation(&installer_profile.manifest.uuid) {
        installation.installed_features = effective_enabled_features.clone();
        installation.enabled_features = effective_enabled_features.clone();
        installation.commit_installation();
        
        installation.installed = true;
        installation.update_available = false;
        installation.modified = false;
        installation.universal_version = installer_profile.manifest.modpack_version.clone();
        
        if let Err(e) = installation.save() {
            error!("Failed to update installation state: {}", e);
            return Err(format!("Failed to save installation state: {}", e));
        }
    }

    // Write error.log in the game directory if anything was skipped, so the
    // user has a short, readable summary instead of needing to dig through
    // installer.log.
    if !all_failures.is_empty() {
        let mut log_contents = format!(
            "Installation completed with {} error(s). The following files could not be \
            downloaded and were skipped:\n\n",
            all_failures.len()
        );
        for failure in &all_failures {
            log_contents.push_str(&format!(
                "- [{}] {}: {}\n",
                failure.kind, failure.name, failure.error
            ));
        }
        log_contents.push_str(
            "\nYou can try reinstalling later, or report these to the modpack \
            maintainers if the problem persists.\n",
        );

        match fs::write(modpack_root.join("error.log"), &log_contents) {
            Ok(_) => warn!(
                "Installation finished with {} skipped item(s); wrote error.log",
                all_failures.len()
            ),
            Err(e) => error!("Failed to write error.log: {}", e),
        }
    }

    info!("Modpack installation completed successfully!");
    Ok(all_failures)
}

// Add these helper functions for downloading includes
async fn download_include_file(
    http_client: &CachedHttpClient,
    url: &str,
    target_path: &Path,
) -> Result<(), String> {
    debug!("Downloading include file from {} to {:?}", url, target_path);
    
    let mut response = http_client.get_async(url).await
        .map_err(|e| format!("Failed to download include: {}", e))?;
    
    if response.status() != StatusCode::OK {
        return Err(format!("Failed to download include: HTTP {}", response.status()));
    }
    
    let bytes = response.bytes().await
        .map_err(|e| format!("Failed to read include bytes: {}", e))?;
    
    fs::write(target_path, bytes)
        .map_err(|e| format!("Failed to write include file: {}", e))?;
    
    debug!("Successfully wrote include file: {:?}", target_path);
    Ok(())
}

async fn download_and_extract_include(
    http_client: &CachedHttpClient,
    zip_url: &str,
    target_path: &Path,
) -> Result<Vec<String>, String> {
    debug!("Downloading and extracting include from {} to {:?}", zip_url, target_path);
    
    let mut response = http_client.get_nocache(zip_url).await
        .map_err(|e| format!("Failed to download include zip: {}", e))?;
    
    if response.status() != StatusCode::OK {
        return Err(format!("Failed to download include zip: HTTP {}", response.status()));
    }
    
    let bytes = response.bytes().await
        .map_err(|e| format!("Failed to read include zip bytes: {}", e))?;
    
    // Create temp file for zip
    let temp_zip = target_path.with_extension("tmp.zip");
    fs::write(&temp_zip, bytes)
        .map_err(|e| format!("Failed to write temp zip: {}", e))?;
    
    // Extract zip
    let file = fs::File::open(&temp_zip)
        .map_err(|e| format!("Failed to open temp zip: {}", e))?;
    
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read zip archive: {}", e))?;
    
    let mut extracted_files = Vec::new();
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {}", e))?;
        
        let outpath = match file.enclosed_name() {
            Some(path) => target_path.join(path),
            None => continue,
        };
        
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)
                        .map_err(|e| format!("Failed to create parent directory: {}", e))?;
                }
            }
            let mut outfile = fs::File::create(&outpath)
                .map_err(|e| format!("Failed to create file: {}", e))?;
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("Failed to extract file: {}", e))?;
            
            extracted_files.push(outpath.to_string_lossy().to_string());
        }
    }
    
    // Remove temp zip
    let _ = fs::remove_file(&temp_zip);
    
    Ok(extracted_files)
}

fn remove_old_items<T: Downloadable + PartialEq + Clone + Debug>(
    items: &[T],
    installed_items: &Vec<T>,
) -> Vec<T> {
    let new_items: Vec<T> = items
        .iter()
        .filter_map(|item| {
            installed_items
                .iter()
                .find(|installed_item| installed_item.get_name() == item.get_name())
                .map_or_else(
                    || Some(item.clone()),
                    |installed_item| {
                        if installed_item.get_version() == item.get_version() {
                            Some(installed_item.clone())
                        } else {
                            if let Some(path) = installed_item.get_path().as_ref() {
                                let _ = fs::remove_file(path);
                            } else {
                                warn!("Missing 'path' field on {installed_item:#?}")
                            }

                            Some(item.clone())
                        }
                    },
                )
        })
        .collect();
    installed_items
        .iter()
        .filter(|x| !new_items.contains(x))
        .for_each(|x| {
            if let Some(path) = x.get_path().as_ref() {
                let _ = fs::remove_file(path);
            } else {
                warn!("Missing 'path' field on {x:#?}")
            }
        });
    new_items
}

// Why haven't I split this into multiple files? That's a good question. I forgot, and I can't be bothered to do it now.
// TODO(Split project into multiple files to improve maintainability)
async fn update<F: FnMut() + Clone>(installer_profile: &InstallerProfile, progress_callback: F)-> Result<Vec<FailedItem>, String> {
    info!("Updating modpack");
    debug!("installer_profile = {installer_profile:#?}");
    let local_manifest: Manifest = match fs::read_to_string(
        get_modpack_root(
            installer_profile
                .launcher
                .as_ref()
                .expect("Launcher not selected!"),
            &installer_profile.manifest.uuid,
        )
        .join(Path::new("manifest.json")),
    ) {
        Ok(contents) => match serde_json::from_str(&contents) {
            Ok(parsed) => parsed,
            Err(err) => panic!("Failed to parse local manifest: {}", err),
        },
        Err(err) => panic!("Failed to read local manifest: {}", err),
    };
    let new_mods = remove_old_items(&installer_profile.manifest.mods, &local_manifest.mods);
    let new_shaderpacks = remove_old_items(
        &installer_profile.manifest.shaderpacks,
        &local_manifest.shaderpacks,
    );
    let new_resourcepacks = remove_old_items(
        &installer_profile.manifest.resourcepacks,
        &local_manifest.resourcepacks,
    );
    let mut update_profile = installer_profile.clone();
    update_profile.manifest.mods = new_mods;
    update_profile.manifest.shaderpacks = new_shaderpacks;
    update_profile.manifest.resourcepacks = new_resourcepacks;


    let e = install(&update_profile, progress_callback).await;
    if e.is_ok() {
        info!("Updated modpack");
    } else {
        error!("Failed to update modpack: {e:#?}")
    }
    e
}

fn get_launcher(string_representation: &str) -> Result<Launcher, String> {
    let mut launcher = string_representation.split('-').collect::<Vec<_>>();

    match *launcher.first().unwrap() {
        "vanilla" => Ok(Launcher::Vanilla(get_app_data())),
        "multimc" => {
            let data_dir = get_multimc_folder(
                launcher
                    .last()
                    .expect("Missing data dir segement in MultiMC!"),
            );
            match data_dir {
                Ok(path) => Ok(Launcher::MultiMC(path)),
                Err(e) => Err(e),
            }
        }
        "custom" => {
            let data_dir = PathBuf::from(launcher.split_off(1).join("-"));
            match data_dir.metadata() {
                Ok(metadata) => {
                    if !metadata.is_dir() || !data_dir.join("instances").is_dir() {
                        return Err(String::from("MultiMC directory is not a valid directory!"));
                    }
                }
                Err(e) => return Err(e.to_string()),
            }
            Ok(Launcher::MultiMC(data_dir))
        }
        _ => Err(String::from("Invalid launcher!")),
    }
}

fn main() {
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
    
    init_tracking();
    
    info!("Installer version: {}", env!("CARGO_PKG_VERSION"));
    let platform_info = PlatformInfo::new().expect("Unable to determine platform info");
    debug!("System information:\n\tSysname: {}\n\tRelease: {}\n\tVersion: {}\n\tArchitecture: {}\n\tOsname: {}",platform_info.sysname().to_string_lossy(), platform_info.release().to_string_lossy(), platform_info.version().to_string_lossy(), platform_info.machine().to_string_lossy(), platform_info.osname().to_string_lossy());
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/dev/dri").exists() {
                // SAFETY: There's potential for race conditions in a multi-threaded context.
                unsafe {
                    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
                }
                warn!("Disabled hardware acceleration as a workaround for NVIDIA driver issues")
            }
    }
    let _icon = image::load_from_memory(include_bytes!("assets/icon.png")).unwrap();
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
    
    // Create app icon and use it immediately
    let app_icon_data = include_bytes!("assets/icon.png");
    let app_icon = image::load_from_memory(app_icon_data).unwrap();
    let window_icon = Icon::from_rgba(
        app_icon.to_rgba8().to_vec(),
        app_icon.width(),
        app_icon.height()
    ).unwrap();
    
    // Launch the UI
    LaunchBuilder::desktop().with_cfg(
        DioxusConfig::new().with_window(
            WindowBuilder::new()
                .with_resizable(true)
                .with_title("Majestic Overhaul Launcher")
                .with_inner_size(LogicalSize::new(1280, 720))
                .with_min_inner_size(LogicalSize::new(960, 540))
        ).with_icon(window_icon)  // Use the icon variable here
        .with_data_directory(
            env::temp_dir().join(".WC_OVHL")
        ).with_menu(None)
    ).with_context(gui::AppProps {
        branches,
        modpack_source: String::from(REPO),
        config,
        config_path,
        installations,
    }).launch(gui::app);
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum Launcher {
    Vanilla(PathBuf),
    MultiMC(PathBuf),
}

#[derive(Deserialize)]
struct GithubContent {
    name: String,
    path: String,
    download_url: Option<String>,
    #[serde(rename = "type")]
    content_type: String,
}

fn download_github_directory<'a>(
    http_client: &'a CachedHttpClient,
    api_url: &'a str,
    relative_path: &'a str,
    modpack_root: &'a Path,
) -> Pin<Box<dyn Future<Output = Result<Vec<String>, String>> + 'a>> {
    Box::pin(async move {
        debug!("Downloading GitHub directory from API: {}", api_url);
        
        // Add GitHub token if available to avoid rate limits
        let mut response = http_client.get_async(api_url).await
            .map_err(|e| format!("Failed to fetch directory listing: {}", e))?;
            
        if response.status() != StatusCode::OK {
            error!("GitHub API returned status {} for URL: {}", response.status(), api_url);
            
            // Check if it's a rate limit issue
            if response.status() == StatusCode::FORBIDDEN {
                error!("Possible GitHub API rate limit hit. Consider adding authentication.");
            }
            
            return Err(format!("GitHub API returned status: {}", response.status()));
        }
        
        let json_text = response.text().await
            .map_err(|e| format!("Failed to read directory listing: {}", e))?;
            
        debug!("Got directory listing response of {} bytes", json_text.len());
            
        let contents: Vec<GithubContent> = serde_json::from_str(&json_text)
            .map_err(|e| {
                error!("Failed to parse JSON: {}", e);
                error!("JSON content: {}", json_text);
                format!("Failed to parse directory listing: {}", e)
            })?;
        
        debug!("Found {} items in directory {}", contents.len(), relative_path);
        
        let mut downloaded_files = Vec::new();
        
        for item in contents {
            debug!("Processing item: {} (type: {})", item.name, item.content_type);
            let target_path = modpack_root.join(&item.path);
            
            if item.content_type == "file" {
                if let Some(download_url) = item.download_url {
                    // Create parent directory
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent)
                            .map_err(|e| format!("Failed to create directory: {}", e))?;
                    }
                    
                    // Download the file
                    debug!("Downloading file: {} -> {:?}", download_url, target_path);
                    let mut file_response = http_client.get_async(&download_url).await
                        .map_err(|e| format!("Failed to download file {}: {}", item.name, e))?;
                        
                    let file_bytes = file_response.bytes().await
                        .map_err(|e| format!("Failed to read file bytes: {}", e))?;
                        
                    fs::write(&target_path, file_bytes)
                        .map_err(|e| format!("Failed to write file: {}", e))?;
                        
                    downloaded_files.push(target_path.to_string_lossy().to_string());
                    debug!("Downloaded file: {}", item.path);
                } else {
                    error!("No download URL for file: {}", item.name);
                }
            } else if item.content_type == "dir" {
                // Create the directory
                fs::create_dir_all(&target_path)
                    .map_err(|e| format!("Failed to create directory {}: {}", target_path.display(), e))?;
                
                // Recursively download subdirectories
                let subdir_url = format!(
                    "https://api.github.com/repos/Wynncraft-Overhaul/majestic-overhaul/contents/{}",
                    item.path
                );
                
                match download_github_directory(http_client, &subdir_url, &item.path, modpack_root).await {
                    Ok(mut subfiles) => {
                        debug!("Downloaded {} files from subdirectory {}", subfiles.len(), item.path);
                        downloaded_files.append(&mut subfiles);
                    },
                    Err(e) => {
                        error!("Failed to download subdirectory {}: {}", item.path, e);
                        // Continue with other files instead of failing completely
                    }
                }
            }
        }
        
        debug!("Downloaded {} files total for directory {}", downloaded_files.len(), relative_path);
        Ok(downloaded_files)
    })
}

impl Display for Launcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Launcher::Vanilla(_) => write!(f, "Vanilla"),
            Launcher::MultiMC(_) => write!(f, "MultiMC"),
        }
    }
}


#[derive(Debug, Clone)]
struct InstallerProfile {
    manifest: Manifest,
    http_client: CachedHttpClient,
    installed: bool,
    update_available: bool,
    modpack_source: String,
    modpack_branch: String,
    enabled_features: Vec<String>,
    launcher: Option<Launcher>,
    local_manifest: Option<Manifest>,
    changelog: Option<Changelog>, // This now uses the imported type
}
           
impl PartialEq for InstallerProfile {
    fn eq(&self, other: &Self) -> bool {
        self.manifest == other.manifest && 
        self.installed == other.installed && 
        self.update_available == other.update_available && 
        self.modpack_source == other.modpack_source && 
        self.modpack_branch == other.modpack_branch && 
        self.enabled_features == other.enabled_features && 
        self.launcher == other.launcher && 
        self.local_manifest == other.local_manifest
        // We're intentionally not comparing changelog for equality
        // as it's not critical for determining if profiles are equal
    }
}

async fn init(
    modpack_source: String,
    modpack_branch: String,
    launcher: Launcher,
) -> Result<InstallerProfile, String> {
    debug!("Initializing with:");
    debug!("  Source: {}", modpack_source);
    debug!("  Branch: {}", modpack_branch);
    debug!("  Launcher: {:?}", launcher);

    // Create http_client first
    let http_client = CachedHttpClient::new();
    
    // Construct full URL for manifest
    let full_url = format!("{}{}{}/manifest.json", GH_RAW, modpack_source, modpack_branch);
    debug!("Fetching manifest from URL: {}", full_url);

    // Fetch manifest
    let mut manifest_resp = match http_client.get_async(full_url.clone()).await {
        Ok(val) => val,
        Err(e) => {
            error!("Failed to fetch manifest. Error: {:?}", e);
            return Err(e.to_string());
        }
    };

    let manifest_text = match manifest_resp.text().await {
        Ok(text) => {
            debug!("Received manifest text");
            text
        },
        Err(e) => {
            error!("Failed to get manifest text. Error: {:?}", e);
            return Err(e.to_string());
        }
    };

    let manifest: Manifest = match serde_json::from_str(&manifest_text) {
        Ok(val) => val,
        Err(e) => {
            error!("Failed to parse manifest. Error: {:?}", e);
            return Err(e.to_string());
        }
    };

    // Its not guaranteed that a manifest with a different version manages to parse however we handle parsing failures and therefore we should be fine to just return an error here
    if CURRENT_MANIFEST_VERSION != manifest.manifest_version {
        return Err(format!(
            "Unsupported manifest version '{}'!",
            manifest.manifest_version
        ));
    }

    // Now try to fetch the changelog
    let full_source = format!("{}{}", modpack_source, modpack_branch);
    let changelog = match crate::changelog::fetch_changelog(&full_source, &http_client).await {
        Ok(changelog) => {
            debug!("Successfully fetched changelog with {} entries", changelog.entries.len());
            Some(changelog)
        },
        Err(e) => {
            // Just log the error but don't fail - changelog is optional
            warn!("Couldn't fetch changelog: {}", e);
            None
        }
    };

    let modpack_root = get_modpack_root(&launcher, &manifest.uuid);
    let mut installed = modpack_root.join(Path::new("manifest.json")).exists();
    let local_manifest: Option<Result<Manifest, serde_json::Error>> = if installed {
        let local_manifest_content =
            match fs::read_to_string(modpack_root.join(Path::new("manifest.json"))) {
                Ok(val) => val,
                Err(e) => return Err(e.to_string()),
            };
        Some(serde_json::from_str(&local_manifest_content))
    } else {
        installed = false;
        None
    };
    let update_available = if installed {
        match local_manifest.as_ref().unwrap() {
            Ok(val) => manifest.modpack_version != val.modpack_version,
            Err(_) => false,
        }
    } else {
        false
    };
    let mut enabled_features = vec![default_id()];
    if !installed {
        for feat in &manifest.features {
            if feat.default {
                enabled_features.push(feat.id.clone());
            }
        }
    }
    Ok(InstallerProfile {
        manifest,
        http_client,
        installed,
        update_available,
        modpack_source,
        modpack_branch,
        enabled_features,
        launcher: Some(launcher),
        local_manifest: if local_manifest.is_some() && local_manifest.as_ref().unwrap().is_ok() {
            Some(local_manifest.unwrap().unwrap())
        } else {
            None
        },
        changelog, // Add the changelog field
    })
}

fn compare_versions(v1: &str, v2: &str) -> std::cmp::Ordering {
    let parse_version = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };
    
    let v1_parts = parse_version(v1);
    let v2_parts = parse_version(v2);
    
    for i in 0..std::cmp::max(v1_parts.len(), v2_parts.len()) {
        let p1 = v1_parts.get(i).copied().unwrap_or(0);
        let p2 = v2_parts.get(i).copied().unwrap_or(0);
        
        match p1.cmp(&p2) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    
    std::cmp::Ordering::Equal
}
