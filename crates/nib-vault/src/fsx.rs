//! The only filesystem mutation boundary; every entry point checks the guard.
use crate::{Generation, Guard, VaultError, guard::resolve};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
static WRITES: Mutex<()> = Mutex::new(());
/// Create a directory hierarchy under the write policy.
pub fn create_dir_all(guard: &Guard, path: &Path) -> Result<(), VaultError> {
    guard.check(path)?;
    fs::create_dir_all(path)?;
    Ok(())
}
/// Atomically replace a file after syncing its data, permissions and parent directory.
/// A process-wide mutex serializes same-stem temporary names and concurrent saves.
pub fn save(guard: &Guard, path: &Path, bytes: &[u8]) -> Result<Generation, VaultError> {
    let _lock = WRITES
        .lock()
        .map_err(|e| VaultError::Message(e.to_string()))?;
    guard.check(path)?;
    let target = resolve(path)?;
    guard.check(&target)?;
    let parent = target
        .parent()
        .ok_or_else(|| VaultError::Denied(target.clone()))?;
    let stem = target
        .file_stem()
        .ok_or_else(|| VaultError::Denied(target.clone()))?
        .to_string_lossy();
    let tmp = parent.join(format!(".{stem}.nib-tmp"));
    guard.check(&tmp)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    let result = (|| -> Result<(), VaultError> {
        file.write_all(bytes)?;
        if let Ok(meta) = fs::metadata(&target) {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        fs::rename(&tmp, &target)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    Ok(NEXT.fetch_add(1, Ordering::Relaxed))
}
/// Create bytes without ever replacing an existing file, including dangling symlinks.
/// Hard-link installation publishes only a fully synced inode.
pub fn create(guard: &Guard, path: &Path, bytes: &[u8]) -> Result<(), VaultError> {
    let _lock = WRITES
        .lock()
        .map_err(|e| VaultError::Message(e.to_string()))?;
    guard.check(path)?;
    if fs::symlink_metadata(path).is_ok() {
        return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists).into());
    }
    let parent = path
        .parent()
        .ok_or_else(|| VaultError::Denied(path.into()))?;
    let tmp = parent.join(format!(
        ".{}.nib-tmp",
        path.file_stem().unwrap_or_default().to_string_lossy()
    ));
    guard.check(&tmp)?;
    let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    let r = (|| -> Result<(), VaultError> {
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::hard_link(&tmp, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(&tmp);
    r
}
/// Rename a guarded entry; callers resolve conflicts before calling.
pub fn move_entry(guard: &Guard, from: &Path, to: &Path) -> Result<(), VaultError> {
    guard.check(from)?;
    guard.check(to)?;
    fs::rename(from, to)?;
    Ok(())
}
/// Remove only old regular nib temporary files, never symlinks or note files.
pub fn cleanup_stale_tmp(
    guard: &Guard,
    folder: &Path,
    now: SystemTime,
) -> Result<Vec<PathBuf>, VaultError> {
    let mut removed = Vec::new();
    for entry in fs::read_dir(folder)? {
        let e = entry?;
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with('.') || !name.ends_with(".nib-tmp") {
            continue;
        }
        let meta = e.path().symlink_metadata()?;
        if meta.is_file()
            && now
                .duration_since(meta.modified()?)
                .unwrap_or(Duration::ZERO)
                > Duration::from_secs(60)
        {
            guard.check(&e.path())?;
            fs::remove_file(e.path())?;
            removed.push(e.path());
        }
    }
    Ok(removed)
}
/// Append diagnostic bytes through the guarded boundary.
pub fn append(guard: &Guard, path: &Path, bytes: &[u8]) -> Result<(), VaultError> {
    guard.check(path)?;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(bytes)?;
    Ok(())
}
/// Apply permissions after checking the resolved target.
pub fn set_permissions(
    guard: &Guard,
    path: &Path,
    permissions: fs::Permissions,
) -> Result<(), VaultError> {
    guard.check(path)?;
    fs::set_permissions(path, permissions)?;
    Ok(())
}
/// Create a symlink after checking both its entry and resolved destination.
pub fn symlink(guard: &Guard, target: &Path, link: &Path) -> Result<(), VaultError> {
    guard.check(link)?;
    let dest = if target.is_absolute() {
        target.to_owned()
    } else {
        link.parent()
            .ok_or_else(|| VaultError::Denied(link.into()))?
            .join(target)
    };
    guard.check(&dest)?;
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}
