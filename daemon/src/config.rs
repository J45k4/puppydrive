use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub schema_version: u32,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub general: GeneralConfig,
    pub backup: BackupConfig,
    pub appearance: AppearanceConfig,
    pub media: MediaConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            general: GeneralConfig::default(),
            backup: BackupConfig::default(),
            appearance: AppearanceConfig::default(),
            media: MediaConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind_address: String,
    pub this_computer_root: PathBuf,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:5777".to_owned(),
            this_computer_root: PathBuf::from("/"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub device_name: String,
    pub start_on_login: bool,
    pub notifications_enabled: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            device_name: "PuppyDrive".to_owned(),
            start_on_login: true,
            notifications_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    pub metered_connections: bool,
    pub schedule: BackupSchedule,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            metered_connections: false,
            schedule: BackupSchedule::Continuous,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupSchedule {
    #[default]
    Continuous,
    Hourly,
    Daily,
}

impl BackupSchedule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Continuous => "continuous",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "continuous" => Some(Self::Continuous),
            "hourly" => Some(Self::Hourly),
            "daily" => Some(Self::Daily),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub theme: Theme,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: Theme::System,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaConfig {
    pub paths_initialized: bool,
    pub max_items: usize,
    pub max_directories: usize,
    pub watch_debounce_ms: u64,
    pub fallback_rescan_seconds: u64,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            paths_initialized: false,
            max_items: 1_000,
            max_directories: 512,
            watch_debounce_ms: 1_000,
            fallback_rescan_seconds: 30,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigPaths {
    pub config_file: PathBuf,
    pub database_file: PathBuf,
}

impl ConfigPaths {
    pub fn discover(config: &AppConfig) -> Result<Self> {
        let project = ProjectDirs::from("com", "Puppy", "PuppyDrive")
            .context("unable to determine per-user PuppyDrive directories")?;
        let config_file = env::var_os("PUPPYDRIVE_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| project.config_dir().join("config.json"));
        let config_dir = config_file.parent().unwrap_or_else(|| Path::new("."));
        let configured_database = config.database.path.clone().map(|path| {
            if path.is_absolute() {
                path
            } else {
                config_dir.join(path)
            }
        });
        let database_file = env::var_os("WGUI_DATABASE_URL")
            .map(PathBuf::from)
            .map(normalize_database_url)
            .transpose()?
            .or(configured_database)
            .unwrap_or_else(|| project.data_local_dir().join("puppydrive.db"));
        Ok(Self {
            config_file,
            database_file,
        })
    }
}

fn normalize_database_url(path: PathBuf) -> Result<PathBuf> {
    let value = path.to_string_lossy();
    if let Some(value) = value.strip_prefix("sqlite://") {
        Ok(PathBuf::from(value))
    } else if let Some(value) = value.strip_prefix("sqlite:") {
        if value == ":memory:" {
            bail!("an in-memory WGUI_DATABASE_URL is not supported")
        }
        Ok(PathBuf::from(value))
    } else if value.contains("://") {
        bail!("WGUI_DATABASE_URL must be a SQLite path or sqlite:// URL")
    } else {
        Ok(path)
    }
}

pub fn load() -> Result<(AppConfig, ConfigPaths)> {
    let provisional = AppConfig::default();
    let provisional_paths = ConfigPaths::discover(&provisional)?;
    let config = if provisional_paths.config_file.exists() {
        let raw = fs::read_to_string(&provisional_paths.config_file).with_context(|| {
            format!(
                "failed reading configuration {}",
                provisional_paths.config_file.display()
            )
        })?;
        serde_json::from_str::<AppConfig>(&raw).with_context(|| {
            format!(
                "invalid PuppyDrive configuration {}; the file was left unchanged",
                provisional_paths.config_file.display()
            )
        })?
    } else {
        provisional
    };
    if config.schema_version != CONFIG_SCHEMA_VERSION {
        bail!(
            "unsupported PuppyDrive configuration schema version {}; expected {}",
            config.schema_version,
            CONFIG_SCHEMA_VERSION
        );
    }
    config
        .server
        .bind_address
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid bind address '{}'", config.server.bind_address))?;
    let paths = ConfigPaths::discover(&config)?;
    Ok((config, paths))
}

pub fn save(config: &AppConfig, path: &Path) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed creating configuration directory {}",
            parent.display()
        )
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.json");
    let temporary = parent.join(format!(".{file_name}.tmp"));
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&temporary)
        .with_context(|| format!("failed creating {}", temporary.display()))?;
    let json = serde_json::to_vec_pretty(config).context("failed serializing configuration")?;
    file.write_all(&json)
        .with_context(|| format!("failed writing {}", temporary.display()))?;
    file.write_all(b"\n")?;
    file.sync_all()
        .with_context(|| format!("failed syncing {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| {
        format!(
            "failed replacing configuration {} with {}",
            path.display(),
            temporary.display()
        )
    })?;
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let restored: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(restored.backup.schedule, BackupSchedule::Continuous);
        assert_eq!(restored.appearance.theme, Theme::System);
    }

    #[test]
    fn save_writes_a_parseable_private_file() {
        let directory =
            std::env::temp_dir().join(format!("puppydrive-config-{}", uuid::Uuid::new_v4()));
        let path = directory.join("config.json");
        let config = AppConfig::default();
        save(&config, &path).unwrap();
        let restored: AppConfig = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(restored.schema_version, CONFIG_SCHEMA_VERSION);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let _ = fs::remove_dir_all(directory);
    }
}
