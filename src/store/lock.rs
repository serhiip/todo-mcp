//! File-lock primitives for list mutation. Provides exclusive lock path and acquire/release
//! so TodoStore business logic stays separate from lock mechanics.

use std::path::Path;

use fs2::FileExt;

use super::StoreError;

pub fn lock_path_for_list(parent_dir: &Path, list_name: &str) -> std::path::PathBuf {
    parent_dir.join(format!("{}.lock", list_name))
}

#[allow(dead_code)]
pub struct ExclusiveLockGuard(pub(crate) std::fs::File);

pub fn acquire_exclusive(lock_path: &Path) -> Result<ExclusiveLockGuard, StoreError> {
    let f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(lock_path)
        .map_err(|e| StoreError::Io(format!("open lock file: {}", e)))?;
    f.lock_exclusive().map_err(|e| StoreError::Io(format!("lock: {}", e)))?;
    Ok(ExclusiveLockGuard(f))
}
