use std::env;
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_HTTP_ADDR: &str = "0.0.0.0:3000";
const DEFAULT_UDP_ADDR: &str = "0.0.0.0:9000";
const DEFAULT_WORKER_TIMEOUT_SECS: u64 = 9;
const DEFAULT_DB_FLUSH_MS: u64 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DatabaseBackend {
    #[default]
    ReDB,
    Sqlite,
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
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            http_address: Self::parse_addr("MASTER_HTTP_ADDR", DEFAULT_HTTP_ADDR),
            udp_address: Self::parse_addr("MASTER_UDP_ADDR", DEFAULT_UDP_ADDR),
            database_backend: Self::parse_db_backend(),
            database_path: Self::parse_non_empty("MASTER_DB_PATH").map(PathBuf::from),
            worker_timeout: Duration::from_secs(Self::parse_u64(
                "MASTER_WORKER_TIMEOUT_SECS",
                DEFAULT_WORKER_TIMEOUT_SECS,
            )),
            database_flush_interval: Duration::from_millis(Self::parse_u64(
                "MASTER_DB_FLUSH_MS",
                DEFAULT_DB_FLUSH_MS,
            )),
            cors_origins: Self::parse_csv("MASTER_CORS_ORIGINS"),
        }
    }

    fn parse_non_empty(key: &str) -> Option<String> {
        env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn parse_addr(key: &str, default: &str) -> Option<String> {
        match env::var(key) {
            Ok(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty()
                    || trimmed.eq_ignore_ascii_case("off")
                    || trimmed.eq_ignore_ascii_case("none")
                    || trimmed.eq_ignore_ascii_case("false")
                {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            Err(_) => Some(default.to_string()),
        }
    }

    fn parse_db_backend() -> DatabaseBackend {
        match Self::parse_non_empty("MASTER_DB_BACKEND")
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("sqlite") => DatabaseBackend::Sqlite,
            _ => DatabaseBackend::ReDB,
        }
    }

    fn parse_u64(key: &str, default: u64) -> u64 {
        Self::parse_non_empty(key)
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|&value| value > 0)
            .unwrap_or(default)
    }

    fn parse_csv(key: &str) -> Vec<String> {
        Self::parse_non_empty(key)
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
}
