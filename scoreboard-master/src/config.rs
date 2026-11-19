use std::env;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_HTTP_ADDR: &str = "0.0.0.0:3000";
const DEFAULT_UDP_ADDR: &str = "0.0.0.0:9000";
const DEFAULT_WORKER_TIMEOUT_SECS: u64 = 9;
const DEFAULT_DB_FLUSH_MS: u64 = 250;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    NonUtf8 { key: &'static str },
    InvalidBackend { value: String },
    InvalidPositiveInt { key: &'static str, value: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonUtf8 { key } => write!(f, "environment variable {key} is not valid UTF-8"),
            Self::InvalidBackend { value } => write!(
                f,
                "invalid MASTER_DB_BACKEND value '{value}': expected 'redb' or 'sqlite'"
            ),
            Self::InvalidPositiveInt { key, value } => write!(
                f,
                "invalid {key} value '{value}': expected a positive integer"
            ),
        }
    }
}

impl Error for ConfigError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DatabaseBackend {
    #[default]
    ReDB,
    Sqlite,
}

pub trait DatabaseConfig {
    fn database_backend(&self) -> DatabaseBackend;

    fn database_path(&self) -> Option<&Path>;
}

impl DatabaseConfig for Config {
    fn database_backend(&self) -> DatabaseBackend {
        self.database_backend
    }

    fn database_path(&self) -> Option<&Path> {
        self.database_path.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub http_address: Option<String>,
    pub udp_address: Option<String>,
    pub database_backend: DatabaseBackend,
    pub database_path: Option<PathBuf>,
    pub worker_timeout: Duration,
    pub database_flush_interval: Duration,
    pub cors_origins: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            http_address: Some(DEFAULT_HTTP_ADDR.to_string()),
            udp_address: Some(DEFAULT_UDP_ADDR.to_string()),
            database_backend: DatabaseBackend::default(),
            database_path: None,
            worker_timeout: Duration::from_secs(DEFAULT_WORKER_TIMEOUT_SECS),
            database_flush_interval: Duration::from_millis(DEFAULT_DB_FLUSH_MS),
            cors_origins: Vec::new(),
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(env::var)
    }

    fn from_lookup<F>(mut lookup: F) -> Result<Self, ConfigError>
    where
        F: FnMut(&'static str) -> Result<String, env::VarError>,
    {
        Ok(Self {
            http_address: Self::parse_addr(&mut lookup, "MASTER_HTTP_ADDR", DEFAULT_HTTP_ADDR)?,
            udp_address: Self::parse_addr(&mut lookup, "MASTER_UDP_ADDR", DEFAULT_UDP_ADDR)?,
            database_backend: Self::parse_db_backend(&mut lookup)?,
            database_path: Self::parse_non_empty(&mut lookup, "MASTER_DB_PATH")?.map(PathBuf::from),
            worker_timeout: Duration::from_secs(Self::parse_positive_u64(
                &mut lookup,
                "MASTER_WORKER_TIMEOUT_SECS",
                DEFAULT_WORKER_TIMEOUT_SECS,
            )?),
            database_flush_interval: Duration::from_millis(Self::parse_positive_u64(
                &mut lookup,
                "MASTER_DB_FLUSH_MS",
                DEFAULT_DB_FLUSH_MS,
            )?),
            cors_origins: Self::parse_csv(&mut lookup, "MASTER_CORS_ORIGINS")?,
        })
    }

    fn parse_non_empty<F>(lookup: &mut F, key: &'static str) -> Result<Option<String>, ConfigError>
    where
        F: FnMut(&'static str) -> Result<String, env::VarError>,
    {
        match lookup(key) {
            Ok(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            Err(env::VarError::NotPresent) => Ok(None),
            Err(env::VarError::NotUnicode(_)) => Err(ConfigError::NonUtf8 { key }),
        }
    }

    fn parse_addr<F>(
        lookup: &mut F,
        key: &'static str,
        default: &str,
    ) -> Result<Option<String>, ConfigError>
    where
        F: FnMut(&'static str) -> Result<String, env::VarError>,
    {
        match lookup(key) {
            Ok(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty()
                    || trimmed.eq_ignore_ascii_case("off")
                    || trimmed.eq_ignore_ascii_case("none")
                    || trimmed.eq_ignore_ascii_case("false")
                {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            Err(env::VarError::NotPresent) => Ok(Some(default.to_string())),
            Err(env::VarError::NotUnicode(_)) => Err(ConfigError::NonUtf8 { key }),
        }
    }

    fn parse_db_backend<F>(lookup: &mut F) -> Result<DatabaseBackend, ConfigError>
    where
        F: FnMut(&'static str) -> Result<String, env::VarError>,
    {
        let Some(raw) = Self::parse_non_empty(lookup, "MASTER_DB_BACKEND")? else {
            return Ok(DatabaseBackend::default());
        };
        match raw.to_ascii_lowercase().as_str() {
            "sqlite" => Ok(DatabaseBackend::Sqlite),
            "redb" => Ok(DatabaseBackend::ReDB),
            _ => Err(ConfigError::InvalidBackend { value: raw }),
        }
    }

    fn parse_positive_u64<F>(
        lookup: &mut F,
        key: &'static str,
        default: u64,
    ) -> Result<u64, ConfigError>
    where
        F: FnMut(&'static str) -> Result<String, env::VarError>,
    {
        let Some(raw) = Self::parse_non_empty(lookup, key)? else {
            return Ok(default);
        };
        let parsed = raw
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidPositiveInt {
                key,
                value: raw.clone(),
            })?;
        if parsed == 0 {
            return Err(ConfigError::InvalidPositiveInt { key, value: raw });
        }
        Ok(parsed)
    }

    fn parse_csv<F>(lookup: &mut F, key: &'static str) -> Result<Vec<String>, ConfigError>
    where
        F: FnMut(&'static str) -> Result<String, env::VarError>,
    {
        Ok(Self::parse_non_empty(lookup, key)?
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default())
    }
}
