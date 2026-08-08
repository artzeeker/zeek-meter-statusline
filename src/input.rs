//! Deserializes the JSON Claude Code sends to the status line command on stdin.
//! Every field is optional: `rate_limits` is absent for non-subscription accounts
//! or before the first API response, `context_window` is null early in a session,
//! and each rate-limit window may be independently absent. See the "Available
//! data" table at https://code.claude.com/docs/en/statusline.
//!
//! v2 adds the rest of that table (`cost`, `effort`, `thinking`, `fast_mode`,
//! `output_style`, `agent`, `vim`, `pr`, `worktree`, `workspace.repo`,
//! `workspace.git_worktree`, and the extra `context_window` fields) behind
//! the same all-optional contract — none of it is present for every session,
//! and some of it (`pr`, `worktree`, `agent`, `vim`, `effort`) only ever
//! shows up in specific session configurations.

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct Input {
    #[serde(default)]
    pub model: Option<Model>,
    #[serde(default)]
    pub workspace: Option<Workspace>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub context_window: Option<ContextWindow>,
    #[serde(default)]
    pub rate_limits: Option<RateLimits>,
    #[serde(default)]
    pub exceeds_200k_tokens: Option<bool>,
    #[serde(default)]
    pub cost: Option<Cost>,
    #[serde(default)]
    pub effort: Option<Effort>,
    #[serde(default)]
    pub thinking: Option<Thinking>,
    #[serde(default)]
    pub fast_mode: Option<bool>,
    #[serde(default)]
    pub output_style: Option<OutputStyle>,
    #[serde(default)]
    pub agent: Option<Agent>,
    #[serde(default)]
    pub vim: Option<Vim>,
    #[serde(default)]
    pub pr: Option<Pr>,
    #[serde(default)]
    pub worktree: Option<Worktree>,
}

#[derive(Debug, Deserialize)]
pub struct Model {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Workspace {
    #[serde(default)]
    pub current_dir: Option<String>,
    #[serde(default)]
    pub git_worktree: Option<String>,
    #[serde(default)]
    pub repo: Option<Repo>,
}

#[derive(Debug, Deserialize)]
pub struct Repo {
    /// Kept for schema completeness (documented field); `repo_segment` only
    /// displays `owner`/`name` today.
    #[allow(dead_code)]
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ContextWindow {
    #[serde(default)]
    pub used_percentage: Option<f64>,
    #[serde(default)]
    pub total_input_tokens: Option<u64>,
    /// Kept for schema completeness; no segment reads output-token count.
    #[allow(dead_code)]
    #[serde(default)]
    pub total_output_tokens: Option<u64>,
    #[serde(default)]
    pub context_window_size: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RateLimits {
    #[serde(default)]
    pub five_hour: Option<RateWindow>,
    #[serde(default)]
    pub seven_day: Option<RateWindow>,
}

#[derive(Debug, Deserialize)]
pub struct RateWindow {
    #[serde(default)]
    pub used_percentage: Option<f64>,
    #[serde(default)]
    pub resets_at: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Cost {
    #[serde(default)]
    pub total_cost_usd: Option<f64>,
    #[serde(default)]
    pub total_duration_ms: Option<f64>,
    /// Kept for schema completeness; `duration_segment` reports wall-clock
    /// duration (`total_duration_ms`), not time spent waiting on the API.
    #[allow(dead_code)]
    #[serde(default)]
    pub total_api_duration_ms: Option<f64>,
    #[serde(default)]
    pub total_lines_added: Option<i64>,
    #[serde(default)]
    pub total_lines_removed: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct Effort {
    #[serde(default)]
    pub level: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Thinking {
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct OutputStyle {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Agent {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Vim {
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Pr {
    #[serde(default)]
    pub number: Option<u64>,
    /// Kept for schema completeness; `pr_segment` shows the number and
    /// review state, not the URL (too long for a status line).
    #[allow(dead_code)]
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub review_state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Worktree {
    #[serde(default)]
    pub name: Option<String>,
    /// Kept for schema completeness; `worktree_segment` shows only the name.
    #[allow(dead_code)]
    #[serde(default)]
    pub branch: Option<String>,
}

impl Input {
    /// Parses `raw` into an `Input`. Empty or malformed input yields the
    /// all-`None` default rather than an error — a status line that goes
    /// blank because of a JSON hiccup is worse than one that shows
    /// "unknown"/"n/a" placeholders.
    pub fn parse(raw: &str) -> Input {
        if raw.trim().is_empty() {
            return Input::default();
        }
        serde_json::from_str(raw).unwrap_or_default()
    }

    pub fn model_name(&self) -> String {
        self.model
            .as_ref()
            .and_then(|m| m.display_name.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// `workspace.current_dir` is preferred; falls back to the top-level `cwd`
    /// field (both carry the same value per the docs, `workspace.current_dir`
    /// is just the documented-preferred one).
    pub fn resolved_cwd(&self) -> Option<String> {
        self.workspace
            .as_ref()
            .and_then(|w| w.current_dir.clone())
            .or_else(|| self.cwd.clone())
    }

    pub fn ctx_used_pct(&self) -> Option<f64> {
        self.context_window.as_ref().and_then(|c| c.used_percentage)
    }

    pub fn five_hour(&self) -> Option<&RateWindow> {
        self.rate_limits.as_ref().and_then(|r| r.five_hour.as_ref())
    }

    pub fn seven_day(&self) -> Option<&RateWindow> {
        self.rate_limits.as_ref().and_then(|r| r.seven_day.as_ref())
    }

    pub fn worktree_name(&self) -> Option<&str> {
        self.worktree
            .as_ref()
            .and_then(|w| w.name.as_deref())
            .or_else(|| {
                self.workspace
                    .as_ref()
                    .and_then(|w| w.git_worktree.as_deref())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_defaults() {
        let input = Input::parse("");
        assert_eq!(input.model_name(), "unknown");
        assert_eq!(input.ctx_used_pct(), None);
    }

    #[test]
    fn malformed_json_yields_defaults() {
        let input = Input::parse("{not json");
        assert_eq!(input.model_name(), "unknown");
    }

    #[test]
    fn parses_full_payload() {
        let raw = r#"{
            "model": {"display_name": "Sonnet 5"},
            "workspace": {"current_dir": "/repo"},
            "context_window": {"used_percentage": 42},
            "rate_limits": {
                "five_hour": {"used_percentage": 60, "resets_at": 1000},
                "seven_day": {"used_percentage": 20, "resets_at": 2000}
            }
        }"#;
        let input = Input::parse(raw);
        assert_eq!(input.model_name(), "Sonnet 5");
        assert_eq!(input.resolved_cwd().as_deref(), Some("/repo"));
        assert_eq!(input.ctx_used_pct(), Some(42.0));
        assert_eq!(input.five_hour().unwrap().used_percentage, Some(60.0));
        assert_eq!(input.seven_day().unwrap().resets_at, Some(2000.0));
    }

    #[test]
    fn missing_rate_limits_is_none() {
        let input = Input::parse(r#"{"model": {"display_name": "Opus"}}"#);
        assert!(input.five_hour().is_none());
        assert!(input.seven_day().is_none());
        assert!(input.ctx_used_pct().is_none());
    }

    #[test]
    fn parses_v2_fields() {
        let raw = r#"{
            "model": {"display_name": "Opus"},
            "cost": {"total_cost_usd": 0.42, "total_duration_ms": 720000, "total_lines_added": 156, "total_lines_removed": 23},
            "effort": {"level": "high"},
            "thinking": {"enabled": true},
            "fast_mode": false,
            "output_style": {"name": "Explanatory"},
            "agent": {"name": "security-reviewer"},
            "vim": {"mode": "NORMAL"},
            "pr": {"number": 1234, "url": "https://example.com/pr/1234", "review_state": "pending"},
            "worktree": {"name": "my-feature", "branch": "worktree-my-feature"},
            "workspace": {"current_dir": "/repo", "git_worktree": "feature-xyz", "repo": {"host": "github.com", "owner": "anthropics", "name": "claude-code"}},
            "context_window": {"used_percentage": 8, "total_input_tokens": 15500, "context_window_size": 200000},
            "exceeds_200k_tokens": false
        }"#;
        let input = Input::parse(raw);
        assert_eq!(input.cost.as_ref().unwrap().total_cost_usd, Some(0.42));
        assert_eq!(
            input.effort.as_ref().unwrap().level.as_deref(),
            Some("high")
        );
        assert_eq!(input.thinking.as_ref().unwrap().enabled, Some(true));
        assert_eq!(input.fast_mode, Some(false));
        assert_eq!(
            input.output_style.as_ref().unwrap().name.as_deref(),
            Some("Explanatory")
        );
        assert_eq!(
            input.agent.as_ref().unwrap().name.as_deref(),
            Some("security-reviewer")
        );
        assert_eq!(input.vim.as_ref().unwrap().mode.as_deref(), Some("NORMAL"));
        assert_eq!(input.pr.as_ref().unwrap().number, Some(1234));
        assert_eq!(input.worktree_name(), Some("my-feature"));
        assert_eq!(
            input
                .workspace
                .as_ref()
                .unwrap()
                .repo
                .as_ref()
                .unwrap()
                .owner
                .as_deref(),
            Some("anthropics")
        );
        assert_eq!(
            input.context_window.as_ref().unwrap().context_window_size,
            Some(200000)
        );
        assert_eq!(input.exceeds_200k_tokens, Some(false));
    }

    #[test]
    fn worktree_name_falls_back_to_workspace_git_worktree() {
        let raw = r#"{"workspace": {"git_worktree": "feature-xyz"}}"#;
        let input = Input::parse(raw);
        assert_eq!(input.worktree_name(), Some("feature-xyz"));
    }

    #[test]
    fn missing_v2_fields_are_none() {
        let input = Input::parse(r#"{"model": {"display_name": "Opus"}}"#);
        assert!(input.cost.is_none());
        assert!(input.effort.is_none());
        assert!(input.pr.is_none());
        assert!(input.worktree_name().is_none());
    }
}
