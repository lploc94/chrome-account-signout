mod locking;
#[cfg(target_os = "macos")]
mod macos;
mod policy;
#[cfg(windows)]
mod windows;
pub use locking::NativeLock;
pub use policy::{Process, ProcessState, classify};
use signout_core::{Error, Result};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
#[derive(Clone, PartialEq, Eq)]
pub struct Installation {
    pub app: PathBuf,
    pub executable: PathBuf,
}
pub fn choose_installation(
    mut candidates: Vec<Installation>,
    owners: &[PathBuf],
) -> Result<Installation> {
    candidates.sort_by(|a, b| a.executable.cmp(&b.executable));
    candidates.dedup_by(|a, b| a.executable == b.executable);
    let owners: HashSet<_> = owners.iter().collect();
    if owners.len() > 1 {
        return Err(Error::AmbiguousChrome);
    }
    if let Some(owner) = owners.into_iter().next() {
        return candidates
            .into_iter()
            .find(|c| &c.executable == owner)
            .ok_or(Error::Process);
    }
    match candidates.len() {
        0 => Err(Error::MissingChrome),
        1 => Ok(candidates.remove(0)),
        _ => Err(Error::AmbiguousChrome),
    }
}
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(windows)]
use windows as native;
pub fn standard_root() -> Result<PathBuf> {
    native::standard_root()
}
pub fn lock_path() -> Result<PathBuf> {
    native::lock_path()
}
pub fn canonical(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).map_err(|_| Error::Path)
}
pub fn discover(root: &Path) -> Result<Installation> {
    native::discover(root)
}
pub struct Chrome {
    pub installation: Installation,
    pub root: PathBuf,
    tracked: HashSet<(u32, u64)>,
}
impl Chrome {
    pub fn new(root: PathBuf) -> Result<Self> {
        let installation = discover(&root)?;
        Ok(Self {
            installation,
            root,
            tracked: HashSet::new(),
        })
    }
    pub fn snapshot(&mut self) -> Result<ProcessState> {
        let records = native::processes()?;
        // A second executable using this root after discovery is a conflict, not a process to ignore.
        for p in &records {
            if native::is_main(p)
                && p.executable != self.installation.executable
                && policy::owns_root(p, &self.root, &canonical)?
            {
                return Err(Error::AmbiguousChrome);
            }
        }
        let state = classify(
            &records,
            &self.installation.executable,
            &self.root,
            &self.tracked,
            &canonical,
        )?;
        self.tracked = state.relevant.clone();
        Ok(state)
    }
    pub fn stopped(&mut self) -> Result<bool> {
        Ok(self.snapshot()?.relevant.is_empty())
    }
    pub fn close(&mut self, cancel: &AtomicBool) -> Result<()> {
        if cancel.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let state = self.snapshot()?;
        for identity in state.main {
            if !native::close(identity, &self.installation)? {
                return Err(Error::Running);
            }
        }
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            };
            if self.stopped()? {
                return Ok(());
            };
            thread::sleep(Duration::from_millis(200));
        }
    }
    pub fn reopen(&self, folder: &str) -> Result<()> {
        if folder.is_empty()
            || folder == "."
            || folder == ".."
            || folder.contains(['/', '\\', '\0', ':'])
        {
            return Err(Error::Path);
        }
        native::reopen(&self.installation, &self.root, folder)
    }
}
