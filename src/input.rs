//! Deserializes the JSON Claude Code sends to the status line command on stdin.
//! Every field is optional: `rate_limits` is absent for non-subscription accounts
//! or before the first API response, `context_window` is null early in a session,
//! and each rate-limit window may be independently absent. See the "Available
//! data" table at https://code.claude.com/docs/en/statusline.

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
}

#[derive(Debug, Deserialize)]
pub struct Model {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Workspace {
    #[serde(default)]
    pub current_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContextWindow {
    #[serde(default)]
    pub used_percentage: Option<f64>,
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

impl Input {
    /// Parses `raw` into an `Input`. Empty or malformed input yields the
    /// all-`None` default rather than an error, matching statusline.js's
    /// `try { JSON.parse(...) } catch { data = {} }` behavior — a status line
    /// that goes blank because of a JSON hiccup is worse than one that shows
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
}
