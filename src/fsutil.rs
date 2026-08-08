//! Small filesystem helpers shared across the modules that edit JSON config
//! files in place (`settings.rs`, `terminal.rs`, `uninstall.rs`).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Builds a sibling backup path: `<name>.bak-<millis>` next to `path`. Used
/// before any in-place edit of a file we don't own outright (Claude Code's
/// `settings.json`, VS Code's `settings.json`), so a bad merge is always
/// recoverable.
pub fn backup_path(path: &Path) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("settings.json")
        .to_string();
    name.push_str(&format!(".bak-{millis}"));
    path.with_file_name(name)
}

/// Copies `path` to a fresh backup and returns the backup's path.
pub fn backup(path: &Path) -> std::io::Result<PathBuf> {
    let backup = backup_path(path);
    std::fs::copy(path, &backup)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_path_appends_bak_suffix() {
        let p = Path::new("/home/user/.claude/settings.json");
        let b = backup_path(p);
        let name = b.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("settings.json.bak-"));
    }
}
