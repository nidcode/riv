//! Config/data directories (XDG / Windows conventions). `RIV_CONFIG_DIR` / `RIV_DATA_DIR` override for tests.

use std::path::PathBuf;

fn project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "riv")
}

pub fn config_dir() -> PathBuf {
    if let Ok(p) = std::env::var("RIV_CONFIG_DIR") {
        return PathBuf::from(p);
    }
    // Spec: $XDG_CONFIG_HOME/riv or ~/.config/riv (also on macOS); %APPDATA%\riv on Windows.
    if cfg!(windows) {
        return project_dirs().map(|d| d.config_dir().to_path_buf()).unwrap_or_else(|| PathBuf::from("riv"));
    }
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME")
        && !x.is_empty()
    {
        return PathBuf::from(x).join("riv");
    }
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(".config").join("riv"))
        .unwrap_or_else(|| PathBuf::from(".riv-config"))
}

pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("RIV_DATA_DIR") {
        return PathBuf::from(p);
    }
    if cfg!(windows) {
        return project_dirs().map(|d| d.data_dir().to_path_buf()).unwrap_or_else(|| PathBuf::from("riv"));
    }
    if let Ok(x) = std::env::var("XDG_DATA_HOME")
        && !x.is_empty()
    {
        return PathBuf::from(x).join("riv");
    }
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(".local").join("share").join("riv"))
        .unwrap_or_else(|| PathBuf::from(".riv-data"))
}

pub fn credentials_path() -> PathBuf {
    config_dir().join("credentials.json")
}
pub fn db_path() -> PathBuf {
    data_dir().join("riv.db")
}
pub fn plans_dir() -> PathBuf {
    data_dir().join("plans")
}
pub fn venues_path() -> PathBuf {
    config_dir().join("venues.json")
}
