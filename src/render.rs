//! Assembles the final status line(s): selects segments per `Config`,
//! applies the active theme/color depth, joins them for `one-line` layout or
//! splits identity-vs-meters across two rows for `two-line`, and drops
//! low-priority segments if the assembled row would overflow `$COLUMNS`.
//!
//! Returns one `String` per output line — `main.rs` `println!`s each in
//! turn, since Claude Code renders every printed line as its own status-line
//! row.

use crate::config::{Config, Layout};
use crate::git::GitInfo;
use crate::input::Input;
use crate::segments::{self, RenderCtx, Segment};
use crate::theme::{self, Role, Theme};
use std::time::{SystemTime, UNIX_EPOCH};

/// Segment names that belong on the "meters" row in two-line layout.
/// Everything else goes on the "identity" row.
const METER_SEGMENTS: [&str; 6] = ["ctx", "5h", "7d", "reset", "tokens", "pet"];

pub fn render(input: &Input, git: Option<&GitInfo>, config: &Config, now_sec: f64) -> Vec<String> {
    let theme = Theme::by_name(&config.theme);
    let depth = config.color_override.unwrap_or_else(theme::detect_depth);
    let ctx = RenderCtx {
        input,
        git,
        config,
        theme: &theme,
        depth,
        now_sec,
    };

    let (five_h, seven_d) = segments::compute_windows(&ctx);
    let all = segments::build_all(&ctx, &five_h, &seven_d);

    match config.layout {
        Layout::OneLine => vec![assemble_row(all, &ctx).unwrap_or_default()],
        Layout::TwoLine => {
            let (meters, identity): (Vec<Segment>, Vec<Segment>) = all
                .into_iter()
                .partition(|s| METER_SEGMENTS.contains(&s.name));
            [assemble_row(identity, &ctx), assemble_row(meters, &ctx)]
                .into_iter()
                .flatten()
                .collect()
        }
    }
}

/// Convenience wrapper used by `main` for the real invocation (now = current
/// time). Split out from `render` so tests can pin `now_sec`.
pub fn render_now(input: &Input, git: Option<&GitInfo>, config: &Config) -> Vec<String> {
    let now_sec = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    render(input, git, config, now_sec)
}

/// Joins `segs` with the configured separator, then — if `$COLUMNS` is set
/// and narrower than the result — drops segments lowest-priority-first
/// until it fits (or only one segment remains). Never truncates mid-glyph:
/// whole segments are dropped, not characters. Returns `None` for an empty
/// input so callers can skip printing an empty row entirely.
fn assemble_row(segs: Vec<Segment>, ctx: &RenderCtx) -> Option<String> {
    let columns = std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok());
    assemble_row_with_columns(segs, ctx, columns)
}

/// The pure, env-independent core of `assemble_row` — split out so tests can
/// exercise the overflow-drop logic without mutating the process-global
/// `COLUMNS` env var (which would race with other tests running in
/// parallel).
fn assemble_row_with_columns(
    mut segs: Vec<Segment>,
    ctx: &RenderCtx,
    columns: Option<usize>,
) -> Option<String> {
    if segs.is_empty() {
        return None;
    }

    if let Some(cols) = columns {
        loop {
            let joined = join_segments(&segs, ctx);
            if display_width(&joined) <= cols || segs.len() <= 1 {
                return Some(joined);
            }
            let drop_idx = segs
                .iter()
                .enumerate()
                .min_by_key(|(_, s)| s.priority)
                .map(|(i, _)| i)
                .expect("segs is non-empty");
            segs.remove(drop_idx);
        }
    } else {
        Some(join_segments(&segs, ctx))
    }
}

fn join_segments(segs: &[Segment], ctx: &RenderCtx) -> String {
    let glyph = ctx.config.separator.glyph();
    let texts: Vec<&str> = segs.iter().map(|s| s.text.as_str()).collect();
    if glyph.is_empty() {
        return texts.join("  ");
    }
    let sep = format!(
        " {}{glyph}{} ",
        ctx.theme.sgr(Role::Dim, ctx.depth),
        theme::reset(ctx.depth)
    );
    texts.join(&sep)
}

/// Strips ANSI escape sequences, for width measurement and for tests that
/// assert on plain content (labels, bars, percentages) without hardcoding
/// color codes.
pub(crate) fn strip_ansi(s: &str) -> String {
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

fn display_width(s: &str) -> usize {
    strip_ansi(s).chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;
    use crate::theme::ColorDepth;

    fn fixture(raw: &str) -> Input {
        Input::parse(raw)
    }

    /// Built from `Config::default()` directly (never `Config::load`), so
    /// tests are deterministic regardless of whatever real config file may
    /// exist at `~/.claude/zeek-meter-statusline.json` on the machine
    /// running them.
    fn no_color_config() -> Config {
        Config {
            color_override: Some(ColorDepth::None),
            ..Config::default()
        }
    }

    #[test]
    fn mid_window_render_matches_expected_bars() {
        // five_hour: used 60%, resets_at chosen so elapsed = 50% -> yellow, marker at idx 5
        // seven_day: used 20%, resets_at chosen so elapsed = 50% -> green, marker at idx 5
        let now = 1_000_000.0;
        let five_resets = now + segments::FIVE_HOUR_SECONDS / 2.0;
        let seven_resets = now + segments::SEVEN_DAY_SECONDS / 2.0;
        let raw = format!(
            r#"{{"model":{{"display_name":"Sonnet 5"}},"context_window":{{"used_percentage":42}},
            "rate_limits":{{"five_hour":{{"used_percentage":60,"resets_at":{five_resets}}},
            "seven_day":{{"used_percentage":20,"resets_at":{seven_resets}}}}}}}"#
        );
        let input = fixture(&raw);
        let mut cfg = no_color_config();
        cfg.nerd_font = false;
        let lines = render(&input, None, &cfg, now);
        assert_eq!(lines.len(), 1);
        let line = strip_ansi(&lines[0]);

        assert!(line.contains("Sonnet 5"));
        assert!(line.contains("ctx [####------] 42%")); // ctx 42% -> 4 filled
        assert!(line.contains("5h [#####|----] 60%")); // 5h pace marker at idx 5
        assert!(line.contains("7d [##---|----] 20%")); // 7d pace marker at idx 5
    }

    #[test]
    fn missing_fields_render_gracefully() {
        let input = fixture(r#"{"model":{"display_name":"Opus"}}"#);
        let mut cfg = no_color_config();
        cfg.nerd_font = false;
        cfg.pet_enabled = false;
        let lines = render(&input, None, &cfg, 1_000_000.0);
        let line = strip_ansi(&lines[0]);
        assert!(line.contains("Opus"));
        assert!(line.contains("ctx [----------] n/a"));
        assert!(line.contains("5h [----------] n/a"));
        assert!(line.contains("7d [----------] n/a"));
    }

    #[test]
    fn nerd_font_uses_icons_and_block_chars() {
        let input =
            fixture(r#"{"model":{"display_name":"Opus"},"context_window":{"used_percentage":10}}"#);
        let mut cfg = no_color_config();
        cfg.nerd_font = true;
        let lines = render(&input, None, &cfg, 1_000_000.0);
        let line = &lines[0];
        assert!(line.contains('\u{F2DB}'));
        assert!(line.contains('\u{F0E4}'));
        assert!(line.contains('\u{2588}') || line.contains('\u{2591}'));
    }

    #[test]
    fn two_line_layout_splits_identity_from_meters() {
        let input =
            fixture(r#"{"model":{"display_name":"Opus"},"context_window":{"used_percentage":10}}"#);
        let mut cfg = no_color_config();
        cfg.nerd_font = false;
        cfg.layout = Layout::TwoLine;
        let lines = render(&input, None, &cfg, 1_000_000.0);
        assert_eq!(lines.len(), 2);
        assert!(strip_ansi(&lines[0]).contains("Opus"));
        assert!(strip_ansi(&lines[1]).contains("ctx"));
    }

    #[test]
    fn columns_overflow_drops_lowest_priority_segment_first() {
        // Exercises assemble_row_with_columns directly (rather than through
        // render()'s env-var-reading assemble_row) so this test doesn't
        // mutate the process-global COLUMNS var and race other tests.
        let now = 1_000_000.0;
        let raw = format!(
            r#"{{"model":{{"display_name":"Opus"}},"context_window":{{"used_percentage":10}},
            "cost":{{"total_cost_usd":0.42}},
            "rate_limits":{{"five_hour":{{"used_percentage":5,"resets_at":{}}}}}}}"#,
            now + segments::FIVE_HOUR_SECONDS / 2.0
        );
        let input = fixture(&raw);
        let mut cfg = no_color_config();
        cfg.nerd_font = false;
        cfg.segments = vec!["model".to_string(), "ctx".to_string(), "cost".to_string()];
        let theme = Theme::by_name(&cfg.theme);
        let ctx = RenderCtx {
            input: &input,
            git: None,
            config: &cfg,
            theme: &theme,
            depth: ColorDepth::None,
            now_sec: now,
        };
        let (five_h, seven_d) = segments::compute_windows(&ctx);
        let all = segments::build_all(&ctx, &five_h, &seven_d);
        let line = assemble_row_with_columns(all, &ctx, Some(20)).unwrap();
        // "cost" has the lowest priority of the three and should be dropped
        // first to fit a narrow terminal; "model" (highest priority) must
        // survive.
        assert!(!strip_ansi(&line).contains('$'));
        assert!(strip_ansi(&line).contains("Opus"));
    }

    #[test]
    fn strip_ansi_removes_escape_sequences() {
        let colored = "\x1b[32mgreen\x1b[0m plain";
        assert_eq!(strip_ansi(colored), "green plain");
    }
}
