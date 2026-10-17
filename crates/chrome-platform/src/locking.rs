use signout_core::{Error, HeldOperation, LockProvider, Result};
use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
};
pub struct NativeLock {
    path: PathBuf,
}
impl NativeLock {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}
struct Guard {
    _file: File,
}
impl HeldOperation for Guard {}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self._file);
    }
}
impl LockProvider for NativeLock {
    fn acquire(&self) -> Result<Box<dyn HeldOperation>> {
        let parent = self.path.parent().ok_or(Error::Path)?;
        std::fs::create_dir_all(parent).map_err(|_| Error::Path)?;
        // Root relocation is allowed, but the lock itself must never redirect.
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x00200000);
        }
        let file = options.open(&self.path).map_err(|_| Error::Path)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if file.metadata().map_err(|_| Error::Path)?.file_attributes() & 0x400 != 0 {
                return Err(Error::Path);
            }
        }
        if !file.metadata().map_err(|_| Error::Path)?.is_file() {
            return Err(Error::Path);
        }
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_| Error::Busy)?;
        Ok(Box::new(Guard { _file: file }))
    }
}
