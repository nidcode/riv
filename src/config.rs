//! User settings in `config.json` (next to the credentials). Environment variables always win.

use crate::error::{Result, RivError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// `ja` or `en`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

pub const KEYS: &[&str] = &["lang"];

pub fn path() -> PathBuf {
    crate::paths::config_dir().join("config.json")
}

impl Config {
    /// Missing or unreadable files give defaults: a broken config must never block the CLI.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn load() -> Self {
        Self::load_from(&path())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        match key {
            "lang" => Ok(self.lang.clone()),
            other => Err(unknown_key(other)),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "lang" => {
                if !matches!(value, "ja" | "en") {
                    return Err(RivError::validation("lang must be `ja` or `en`"));
                }
                self.lang = Some(value.to_string());
                Ok(())
            }
            other => Err(unknown_key(other)),
        }
    }

    pub fn unset(&mut self, key: &str) -> Result<()> {
        match key {
            "lang" => {
                self.lang = None;
                Ok(())
            }
            other => Err(unknown_key(other)),
        }
    }
}

fn unknown_key(k: &str) -> RivError {
    RivError::validation(format!("unknown setting `{k}` (known: {})", KEYS.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_unset_roundtrip_on_disk() {
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path().join("sub/config.json");
        let mut c = Config::load_from(&p);
        assert_eq!(c.get("lang").expect("get"), None);
        c.set("lang", "ja").expect("set");
        c.save_to(&p).expect("save");
        assert_eq!(Config::load_from(&p).get("lang").expect("get").as_deref(), Some("ja"));
        c.unset("lang").expect("unset");
        assert_eq!(c.lang, None);
    }

    #[test]
    fn rejects_bad_values_and_keys_and_survives_garbage() {
        let mut c = Config::default();
        assert!(c.set("lang", "fr").is_err());
        assert!(c.set("nope", "x").is_err());
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path().join("config.json");
        std::fs::write(&p, "{ not json").expect("write");
        assert_eq!(Config::load_from(&p), Config::default());
    }
}
