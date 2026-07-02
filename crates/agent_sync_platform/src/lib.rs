use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformPaths {
    pub home: PathBuf,
    pub config_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
}

pub fn current_platform_paths() -> PlatformPaths {
    PlatformPaths {
        home: std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
        config_dir: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        data_dir: std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
    }
}
