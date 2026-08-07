//! Assembles the final status line: model, git branch, context bar, 5h/7d
//! rate-limit bars, and the mood face, using the exact Nerd Font codepoints
//! (Font Awesome glyphs, present in the `NerdFontsSymbolsOnly` pack this
//! project installs).

use crate::bar::{
    build_bar, color_for_context, color_for_pace, fmt_pct, Color, BAR_WIDTH, DIM, RESET,
};
use crate::git::GitInfo;
use crate::input::{Input, RateWindow};
use std::time::{SystemTime, UNIX_EPOCH};

const ICON_MODEL: char = '\u{F2DB}'; // microchip
const ICON_GIT: char = '\u{F418}'; // git-branch
const ICON_CTX: char = '\u{F0E4}'; // dashboard
const ICON_5H: char = '\u{F017}'; // clock
const ICON_7D: char = '\u{F133}'; // calendar

const FIVE_HOUR_SECONDS: f64 = 18_000.0;
const SEVEN_DAY_SECONDS: f64 = 604_800.0;

struct WindowResult {
    seg: String,
    used_pct: Option<f64>,
    elapsed_pct: Option<f64>,
}

pub fn render(input: &Input, git: Option<&GitInfo>, nerd: bool, now_sec: f64) -> String {
    let mut segments = Vec::new();

    segments.push(model_segment(input, nerd));
    if let Some(g) = git {
        segments.push(git_segment(g, nerd));
    }

    let ctx_pct = input.ctx_used_pct();
    segments.push(context_segment(ctx_pct, nerd));

    let five_h = window_segment(
        input.five_hour(),
        FIVE_HOUR_SECONDS,
        ICON_5H,
        "5h",
        nerd,
        now_sec,
    );
    let seven_d = window_segment(
        input.seven_day(),
        SEVEN_DAY_SECONDS,
        ICON_7D,
        "7d",
        nerd,
        now_sec,
    );
    segments.push(five_h.seg.clone());
    segments.push(seven_d.seg.clone());

    segments.push(pet_face(ctx_pct, &five_h).to_string());

    let sep = format!(" {DIM}|{RESET} ");
    segments.join(&sep)
}

/// Convenience wrapper used by `main` for the real invocation (now = current
/// time). Split out from `render` so tests can pin `now_sec`.
pub fn render_now(input: &Input, git: Option<&GitInfo>, nerd: bool) -> String {
    let now_sec = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    render(input, git, nerd, now_sec)
}

fn model_segment(input: &Input, nerd: bool) -> String {
    let icon = if nerd {
        format!("{ICON_MODEL} ")
    } else {
        String::new()
    };
    format!("{icon}{}", input.model_name())
}

fn git_segment(git: &GitInfo, nerd: bool) -> String {
    let icon = if nerd {
        format!("{ICON_GIT} ")
    } else {
        String::new()
    };
    let dirty_marker = if git.dirty { "*" } else { "" };
    format!("{icon}{}{dirty_marker}", git.branch)
}

fn context_segment(ctx_pct: Option<f64>, nerd: bool) -> String {
    let icon = if nerd {
        format!("{ICON_CTX} ")
    } else {
        "ctx ".to_string()
    };
    let color = color_for_context(ctx_pct);
    let bar = build_bar(BAR_WIDTH, ctx_pct, None, nerd);
    format!("{icon}{}[{bar}]{RESET} {}", color.ansi(), fmt_pct(ctx_pct))
}

fn window_segment(
    rl: Option<&RateWindow>,
    total_seconds: f64,
    icon_char: char,
    label: &str,
    nerd: bool,
    now_sec: f64,
) -> WindowResult {
    let icon = if nerd {
        format!("{icon_char} ")
    } else {
        format!("{label} ")
    };
    let used_pct = rl.and_then(|w| w.used_percentage);

    let Some(used_pct) = used_pct else {
        let bar = build_bar(BAR_WIDTH, None, None, nerd);
        return WindowResult {
            seg: format!("{icon}{}[{bar}]{RESET} n/a", Color::Gray.ansi()),
            used_pct: None,
            elapsed_pct: None,
        };
    };

    let resets_at = rl.and_then(|w| w.resets_at);
    let (elapsed_pct, pace_idx) = match resets_at {
        Some(resets_at) => {
            let elapsed_sec = total_seconds - (resets_at - now_sec);
            let e = (elapsed_sec / total_seconds * 100.0).clamp(0.0, 100.0);
            let idx = ((e / 100.0) * BAR_WIDTH as f64).round() as usize;
            (Some(e), Some(idx))
        }
        None => (None, None),
    };

    let color = color_for_pace(Some(used_pct), elapsed_pct);
    let bar = build_bar(BAR_WIDTH, Some(used_pct), pace_idx, nerd);
    WindowResult {
        seg: format!(
            "{icon}{}[{bar}]{RESET} {}",
            color.ansi(),
            fmt_pct(Some(used_pct))
        ),
        used_pct: Some(used_pct),
        elapsed_pct,
    }
}

/// Mood reflects the worse of context-window usage and how far the 5h bar is
/// running ahead of its own pace marker (never how far *behind*, since being
/// behind pace isn't something to be stressed about).
fn pet_face(ctx_pct: Option<f64>, five_h: &WindowResult) -> &'static str {
    let five_h_ahead = match (five_h.used_pct, five_h.elapsed_pct) {
        (Some(u), Some(e)) => (u - e).max(0.0),
        _ => 0.0,
    };
    let worst = ctx_pct.unwrap_or(0.0).max(five_h_ahead);

    if worst < 15.0 {
        ":)"
    } else if worst < 40.0 {
        ":/"
    } else if worst < 70.0 {
        ">:("
    } else {
        "X_X"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;

    fn fixture(raw: &str) -> Input {
        Input::parse(raw)
    }

    /// Strips ANSI escape sequences so assertions can check plain content
    /// (labels, bars, percentages) without hardcoding color codes.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Consume through the final byte of the CSI sequence ('m' for
                // the SGR codes this renderer emits).
                for c2 in chars.by_ref() {
                    if c2 == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn mid_window_render_matches_expected_bars() {
        // five_hour: used 60%, resets_at chosen so elapsed = 50% -> yellow, marker at idx 5
        // seven_day: used 20%, resets_at chosen so elapsed = 50% -> green, marker at idx 5
        let now = 1_000_000.0;
        let five_resets = now + FIVE_HOUR_SECONDS / 2.0;
        let seven_resets = now + SEVEN_DAY_SECONDS / 2.0;
        let raw = format!(
            r#"{{"model":{{"display_name":"Sonnet 5"}},"context_window":{{"used_percentage":42}},
            "rate_limits":{{"five_hour":{{"used_percentage":60,"resets_at":{five_resets}}},
            "seven_day":{{"used_percentage":20,"resets_at":{seven_resets}}}}}}}"#
        );
        let input = fixture(&raw);
        let line = strip_ansi(&render(&input, None, false, now));

        assert!(line.contains("Sonnet 5"));
        assert!(line.contains("ctx [####------] 42%")); // ctx 42% -> 4 filled
        assert!(line.contains("5h [#####|----] 60%")); // 5h pace marker at idx 5
        assert!(line.contains("7d [##---|----] 20%")); // 7d pace marker at idx 5
        assert!(line.ends_with(">:(")); // worst = max(42, 60-50=10) = 42 -> stressed
    }

    #[test]
    fn missing_fields_render_gracefully() {
        let input = fixture(r#"{"model":{"display_name":"Opus"}}"#);
        let line = strip_ansi(&render(&input, None, false, 1_000_000.0));
        assert!(line.contains("Opus"));
        assert!(line.contains("ctx [----------] n/a"));
        assert!(line.contains("5h [----------] n/a"));
        assert!(line.contains("7d [----------] n/a"));
        assert!(line.ends_with(":)")); // worst = 0 -> calm
    }

    #[test]
    fn overheated_case() {
        let now = 1_000_000.0;
        let five_resets = now + FIVE_HOUR_SECONDS * 0.11; // ~11% elapsed
        let seven_resets = now + SEVEN_DAY_SECONDS * 0.08; // ~92% elapsed
        let raw = format!(
            r#"{{"model":{{"display_name":"Opus"}},"context_window":{{"used_percentage":85}},
            "rate_limits":{{"five_hour":{{"used_percentage":95,"resets_at":{five_resets}}},
            "seven_day":{{"used_percentage":10,"resets_at":{seven_resets}}}}}}}"#
        );
        let input = fixture(&raw);
        let line = render(&input, None, false, now);
        assert!(line.ends_with("X_X"));
    }

    #[test]
    fn nerd_font_uses_icons_and_block_chars() {
        let input =
            fixture(r#"{"model":{"display_name":"Opus"},"context_window":{"used_percentage":10}}"#);
        let line = render(&input, None, true, 1_000_000.0);
        assert!(line.contains(ICON_MODEL));
        assert!(line.contains(ICON_CTX));
        assert!(line.contains('█') || line.contains('░'));
    }
}
