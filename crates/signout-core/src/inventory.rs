use crate::{Account, Error, Profile, Result, token_store::TokenStore};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};
#[derive(Clone)]
pub struct Inventory {
    root: PathBuf,
}
fn redirected(path: &Path) -> Result<bool> {
    let m = fs::symlink_metadata(path).map_err(|_| Error::Path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        Ok(m.file_attributes() & 0x400 != 0)
    }
    #[cfg(not(windows))]
    {
        Ok(m.file_type().is_symlink())
    }
}
/// Root may be relocated. All components below the resolved root must be ordinary files/directories.
pub fn safe_child(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.as_os_str().is_empty() {
        return Err(Error::Path);
    }
    let mut current = root.to_path_buf();
    for c in relative.components() {
        match c {
            Component::Normal(v) => current.push(v),
            _ => return Err(Error::Path),
        };
        if redirected(&current)? {
            return Err(Error::Path);
        }
    }
    let resolved = fs::canonicalize(&current).map_err(|_| Error::Path)?;
    // Both sides share the canonical root and actual on-disk component spelling. On Windows
    // canonicalize applies a verbatim prefix consistently; a mismatch is refused, never guessed.
    let expected = root.join(relative);
    if resolved != expected {
        return Err(Error::Path);
    }
    Ok(resolved)
}
impl Inventory {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            root: fs::canonicalize(root).map_err(|_| Error::Path)?,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn safe_file(&self, folder: &str, name: &str) -> Result<PathBuf> {
        if folder.is_empty()
            || folder == "."
            || folder == ".."
            || folder.contains(['/', '\\', '\0', ':'])
            || !matches!(name, "Preferences" | "Web Data")
        {
            return Err(Error::Path);
        }
        safe_child(&self.root, &Path::new(folder).join(name))
    }
    fn cache(&self) -> Result<serde_json::Map<String, Value>> {
        let path = safe_child(&self.root, Path::new("Local State"))?;
        let v: Value = serde_json::from_slice(&fs::read(path).map_err(|_| Error::Metadata)?)
            .map_err(|_| Error::Metadata)?;
        v.pointer("/profile/info_cache")
            .and_then(Value::as_object)
            .cloned()
            .ok_or(Error::Metadata)
    }
    pub fn metadata(&self, folder: &str) -> Result<Profile> {
        let cache = self.cache()?;
        self.metadata_from(folder, cache.get(folder).ok_or(Error::Metadata)?)
    }
    fn metadata_from(&self, folder: &str, info: &Value) -> Result<Profile> {
        let primary = info["gaia_id"]
            .as_str()
            .filter(|s| !s.is_empty() && !s.contains('\0'))
            .ok_or(Error::Identity)?;
        let v: Value = serde_json::from_slice(
            &fs::read(self.safe_file(folder, "Preferences")?).map_err(|_| Error::Metadata)?,
        )
        .map_err(|_| Error::Metadata)?;
        let rows = v["account_info"].as_array().ok_or(Error::Metadata)?;
        let mut seen = HashSet::new();
        let mut accounts = Vec::new();
        for row in rows {
            let gaia = row["gaia"]
                .as_str()
                .filter(|s| !s.is_empty() && !s.contains('\0'))
                .ok_or(Error::Identity)?;
            let email = row["email"]
                .as_str()
                .filter(|s| !s.is_empty() && !s.contains('\0'))
                .ok_or(Error::Identity)?;
            if row["account_id"].as_str() != Some(gaia) || !seen.insert(gaia) {
                return Err(Error::Identity);
            }
            accounts.push(Account {
                gaia_id: gaia.into(),
                email: email.into(),
                name: row["full_name"].as_str().unwrap_or(email).into(),
                is_primary: gaia == primary,
            });
        }
        let owner = accounts
            .iter()
            .find(|a| a.is_primary)
            .ok_or(Error::Identity)?;
        if !info["user_name"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(&owner.email))
        {
            return Err(Error::Identity);
        }
        Ok(Profile {
            folder: folder.into(),
            name: info["name"].as_str().unwrap_or(folder).into(),
            primary_gaia_id: primary.into(),
            accounts,
            unavailable_reason: None,
        })
    }
    /// Confirmed token inventory for callers that require readable Web Data.
    pub fn load(&self) -> Result<Vec<Profile>> {
        self.load_profiles(true)
    }
    /// Display-only cached metadata. Presence here never authorizes a database write.
    /// Chrome may keep Web Data locked; mutation revalidates after it has stopped.
    pub fn load_metadata(&self) -> Result<Vec<Profile>> {
        self.load_profiles(false)
    }
    fn load_profiles(&self, check_tokens: bool) -> Result<Vec<Profile>> {
        let cache = self.cache()?;
        let mut folders: Vec<_> = cache.keys().collect();
        folders.sort();
        let mut result = Vec::new();
        for folder in folders {
            let loaded = (|| {
                let mut p = self.metadata_from(folder, &cache[folder])?;
                if check_tokens {
                    let db = TokenStore::open(&self.safe_file(folder, "Web Data")?, false)?;
                    let services = db.services()?;
                    p.accounts
                        .retain(|a| services.contains(&format!("AccountId-{}", a.gaia_id)));
                }
                Ok(p)
            })();
            result.push(loaded.unwrap_or_else(|e: Error| Profile {
                folder: folder.clone(),
                name: cache[folder]["name"].as_str().unwrap_or(folder).into(),
                primary_gaia_id: String::new(),
                accounts: vec![],
                unavailable_reason: Some(e.to_string()),
            }));
        }
        Ok(result)
    }
}
