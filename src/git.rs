//! Git branch and dirty-flag detection.
//!
//! Branch name is read directly from `.git/HEAD` (a file read, microseconds)
//! instead of spawning `git branch --show-current` (~80ms measured on this
//! machine) — see the "Two performance wins" section of the project plan.
//! This also sidesteps statusline.js's `"## No commits yet on <branch>"`
//! special case entirely: `.git/HEAD` already points at the branch ref even
//! before the first commit exists.
//!
//! The dirty flag genuinely needs git's index comparison, so it still shells
//! out to `git status --porcelain`, but the result is cached to a temp file
//! keyed by `session_id` (stable for a session's lifetime, unique per
//! session — unlike a PID, which changes every invocation) with a short TTL,
//! per the caching pattern in the Claude Code statusline docs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

pub struct GitInfo {
    pub branch: String,
    pub dirty: bool,
}

const DIRTY_CACHE_TTL: Duration = Duration::from_secs(2);

pub fn git_info(cwd: &Path, session_id: Option<&str>) -> Option<GitInfo> {
    let git_dir = find_git_dir(cwd)?;
    let branch = read_branch(&git_dir)?;
    let dirty = dirty_cached(cwd, session_id);
    Some(GitInfo { branch, dirty })
}

/// Walks up from `start` looking for a `.git` directory (normal repo) or a
/// `.git` file containing `gitdir: <path>` (linked worktree).
fn find_git_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join(".git");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if candidate.is_file() {
            if let Ok(contents) = fs::read_to_string(&candidate) {
                if let Some(rest) = contents.trim().strip_prefix("gitdir:") {
                    let p = PathBuf::from(rest.trim());
                    return Some(if p.is_absolute() { p } else { dir.join(p) });
                }
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn read_branch(git_dir: &Path) -> Option<String> {
    let contents = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let contents = contents.trim();

    if let Some(rest) = contents.strip_prefix("ref: refs/heads/") {
        return Some(rest.to_string());
    }
    if let Some(rest) = contents.strip_prefix("ref: ") {
        // Non-branch ref namespace: fall back to the last path segment.
        return Some(rest.rsplit('/').next().unwrap_or(rest).to_string());
    }
    // Detached HEAD: HEAD holds the full 40-char sha directly. Take a
    // 7-char short form (git's common default length; we don't shell out
    // to `git rev-parse --short` to disambiguate, trading a little fidelity
    // for staying off the hot-path subprocess spawn).
    if contents.len() >= 7 && contents.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(contents[..7].to_string());
    }
    None
}

fn dirty_cached(cwd: &Path, session_id: Option<&str>) -> bool {
    let cache_path = cache_file_path(session_id, cwd);

    if let Some(ref path) = cache_path {
        if let Ok(meta) = fs::metadata(path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(age) = SystemTime::now().duration_since(modified) {
                    if age < DIRTY_CACHE_TTL {
                        if let Ok(contents) = fs::read_to_string(path) {
                            return contents.trim() == "1";
                        }
                    }
                }
            }
        }
    }

    let dirty = run_git_status_dirty(cwd);
    if let Some(path) = cache_path {
        let _ = fs::write(path, if dirty { "1" } else { "0" });
    }
    dirty
}

/// Cache key includes both `session_id` (so caches don't leak across
/// sessions/machines sharing a temp dir) and a hash of `cwd` (so caches
/// don't collide across different repos/directories within one session —
/// e.g. after `/add-dir`, a worktree switch, or simply because `session_id`
/// is absent and every invocation would otherwise fall back to one shared
/// key regardless of directory).
fn cache_file_path(session_id: Option<&str>, cwd: &Path) -> Option<PathBuf> {
    let key = sanitize_key(session_id.unwrap_or("no-session"));
    let cwd_hash = hash_path(cwd);
    Some(std::env::temp_dir().join(format!("zeek-meter-statusline-git-{key}-{cwd_hash:x}")))
}

fn hash_path(cwd: &Path) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    cwd.hash(&mut hasher);
    hasher.finish()
}

fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn run_git_status_dirty(cwd: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn unique_tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zeek-meter-statusline-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn no_git_dir_returns_none() {
        let dir = unique_tmp_dir("no-git");
        assert!(find_git_dir(&dir).is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_branch_from_normal_ref() {
        let dir = unique_tmp_dir("normal-ref");
        let git_dir = dir.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(read_branch(&git_dir).as_deref(), Some("main"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_branch_before_first_commit() {
        // No-commits-yet repos still point HEAD at the branch ref, even
        // though refs/heads/main doesn't exist as a file yet.
        let dir = unique_tmp_dir("no-commits");
        let git_dir = dir.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(read_branch(&git_dir).as_deref(), Some("main"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detached_head_yields_short_sha() {
        let dir = unique_tmp_dir("detached");
        let git_dir = dir.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(
            git_dir.join("HEAD"),
            "abcdef0123456789abcdef0123456789abcdef01\n",
        )
        .unwrap();
        assert_eq!(read_branch(&git_dir).as_deref(), Some("abcdef0"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn worktree_gitdir_file_is_followed() {
        let dir = unique_tmp_dir("worktree");
        let real_git_dir = dir.join("real-git-dir");
        fs::create_dir_all(&real_git_dir).unwrap();
        fs::write(real_git_dir.join("HEAD"), "ref: refs/heads/feature\n").unwrap();
        let worktree_dir = dir.join("worktree");
        fs::create_dir_all(&worktree_dir).unwrap();
        fs::write(
            worktree_dir.join(".git"),
            format!("gitdir: {}\n", real_git_dir.display()),
        )
        .unwrap();

        let found = find_git_dir(&worktree_dir).unwrap();
        assert_eq!(read_branch(&found).as_deref(), Some("feature"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sanitize_key_strips_unsafe_chars() {
        assert_eq!(sanitize_key("abc-123"), "abc-123");
        assert_eq!(sanitize_key("a/b c"), "a_b_c");
    }

    #[test]
    fn cache_key_differs_by_cwd_even_with_same_session_id() {
        // Regression test: an earlier version keyed the dirty-flag cache by
        // session_id alone, so two different directories in the same (or
        // absent) session shared one cache file and leaked each other's
        // dirty state. Caught by tests/parity.sh, which ran fixtures
        // pointing at different repos back-to-back within the cache TTL.
        let a = cache_file_path(Some("session-1"), Path::new("/repo/a"));
        let b = cache_file_path(Some("session-1"), Path::new("/repo/b"));
        assert_ne!(a, b);

        let c = cache_file_path(None, Path::new("/repo/a"));
        let d = cache_file_path(None, Path::new("/repo/b"));
        assert_ne!(c, d);
    }
}
