//! Token persistence. `TokenStore` keeps room for OS-keychain backends later.

use crate::error::{Result, RivError};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    /// Unix seconds at which the access token expires.
    pub expires_at: i64,
}

// Never leak tokens through `{:?}`.
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials").field("expires_at", &self.expires_at).finish_non_exhaustive()
    }
}

pub trait TokenStore: Send + Sync {
    fn load(&self) -> Result<Option<Credentials>>;
    fn save(&self, c: &Credentials) -> Result<()>;
    fn clear(&self) -> Result<()>;
}

pub struct FileTokenStore {
    path: PathBuf,
}

impl FileTokenStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn default_location() -> Self {
        Self::new(crate::paths::credentials_path())
    }
}

impl TokenStore for FileTokenStore {
    fn load(&self) -> Result<Option<Credentials>> {
        match std::fs::read(&self.path) {
            Ok(b) => serde_json::from_slice(&b)
                .map(Some)
                .map_err(|_| RivError::auth("credentials file is corrupt; run `riv login` again")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn save(&self, c: &Credentials) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let bytes = serde_json::to_vec_pretty(c)?;
        write_private(&self.path, &bytes)
    }

    fn clear(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    f.write_all(bytes)?;
    // An existing file keeps its old mode on open; enforce 0600 explicitly.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    // Windows: the file lives under the user profile; see README for the caveat.
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn saves_with_0600_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let s = FileTokenStore::new(dir.path().join("sub/credentials.json"));
        let c = Credentials { access_token: "a".into(), refresh_token: Some("r".into()), id_token: None, expires_at: 5 };
        s.save(&c).unwrap();
        let mode = std::fs::metadata(dir.path().join("sub/credentials.json")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(s.load().unwrap().unwrap().access_token, "a");
        assert!(!format!("{c:?}").contains('a') || !format!("{c:?}").contains("access_token"));
        s.clear().unwrap();
        assert!(s.load().unwrap().is_none());
    }
}
