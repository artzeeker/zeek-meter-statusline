//! One builder per status-line segment. `render.rs` selects which of these
//! run (and in what order) from `Config::segments`, then joins or lays them
//! out per `Config::layout`.
//!
//! The original five segments (model, git, ctx, 5h, 7d) plus the pet keep
//! their Nerd Font icons — those codepoints are verified against the
//! `NerdFontsSymbolsOnly` pack the installer ships (see the glyph test both
//! installers print). The v2 segments below deliberately use plain text
//! labels instead of new icons: introducing unverified codepoints risks
//! tofu boxes exactly like the problem the font installer exists to avoid.

use crate::bar::{build_bar, color_for_context, color_for_pace, colorize_bar, fmt_pct};
use crate::config::Config;
use crate::git::GitInfo;
use crate::input::{Input, RateWindow};
use crate::pet::{self, PetInputs};
use crate::theme::{reset, ColorDepth, Role, Theme};

const ICON_MODEL: char = '\u{F2DB}'; // microchip
const ICON_GIT: char = '\u{F418}'; // git-branch
const ICON_CTX: char = '\u{F0E4}'; // dashboard
const ICON_5H: char = '\u{F017}'; // clock
const ICON_7D: char = '\u{F133}'; // calendar

pub const FIVE_HOUR_SECONDS: f64 = 18_000.0;
pub const SEVEN_DAY_SECONDS: f64 = 604_800.0;

/// A "just reset" window is treated as a celebration trigger for this long
/// after `resets_at` passes.
const JUST_RESET_WINDOW_SECONDS: f64 = 120.0;

pub struct Segment {
    pub name: &'static str,
    pub text: String,
    /// Higher survives longer when a row has to shrink to fit `$COLUMNS`.
    pub priority: u8,
}

/// Bundles everything a segment builder needs. Built once per render by
/// `render.rs` and threaded through.
pub struct RenderCtx<'a> {
    pub input: &'a Input,
    pub git: Option<&'a GitInfo>,
    pub config: &'a Config,
    pub theme: &'a Theme,
    pub depth: ColorDepth,
    pub now_sec: f64,
}

pub struct WindowResult {
    pub seg: String,
    pub used_pct: Option<f64>,
    pub elapsed_pct: Option<f64>,
    pub just_reset: bool,
}

fn colorize(text: &str, role: Role, ctx: &RenderCtx) -> String {
    format!(
        "{}{text}{}",
        ctx.theme.sgr(role, ctx.depth),
        reset(ctx.depth)
    )
}

fn priority_of(name: &str) -> u8 {
    match name {
        "model" => 100,
        "ctx" => 95,
        "5h" => 90,
        "7d" => 88,
        "git" => 80,
        "pr" => 75,
        "pet" => 70,
        "reset" => 60,
        "tokens" => 58,
        "cost" => 55,
        "mode" => 50,
        "effort" => 48,
        "worktree" => 45,
        "repo" => 42,
        "duration" => 40,
        "lines" => 38,
        _ => 30,
    }
}

// ---------------------------------------------------------------------------
// Original five + pet
// ---------------------------------------------------------------------------

pub fn model_segment(ctx: &RenderCtx) -> String {
    let icon = if ctx.config.nerd_font {
        format!("{ICON_MODEL} ")
    } else {
        String::new()
    };
    colorize(
        &format!("{icon}{}", ctx.input.model_name()),
        Role::Text,
        ctx,
    )
}

pub fn git_segment(git: &GitInfo, ctx: &RenderCtx) -> String {
    let icon = if ctx.config.nerd_font {
        format!("{ICON_GIT} ")
    } else {
        String::new()
    };
    let branch = colorize(&format!("{icon}{}", git.branch), Role::Text, ctx);
    if git.dirty {
        format!("{branch}{}", colorize("*", Role::Warn, ctx))
    } else {
        branch
    }
}

pub fn context_segment(ctx: &RenderCtx) -> String {
    let ctx_pct = ctx.input.ctx_used_pct();
    let icon = if ctx.config.nerd_font {
        format!("{ICON_CTX} ")
    } else {
        "ctx ".to_string()
    };
    let role = color_for_context(ctx_pct);
    let bar_width = ctx.config.bar_width;
    let bar = build_bar(bar_width, ctx_pct, None, ctx.config.nerd_font);
    let bar = colorize_bar(
        &bar,
        bar_width,
        None,
        ctx.config.nerd_font,
        ctx.theme,
        ctx.depth,
        role,
    );
    let pct = fmt_pct(ctx_pct, ctx.config.percent_decimals);
    format!(
        "{}{icon}{}[{bar}] {pct}{}",
        ctx.theme.sgr(role, ctx.depth),
        reset(ctx.depth),
        reset(ctx.depth)
    )
}

/// Builds one rate-limit window segment (5h or 7d) and returns both its
/// rendered text and the raw numbers other segments (`pet`, `reset`) need.
fn window_segment(
    rl: Option<&RateWindow>,
    total_seconds: f64,
    icon_char: char,
    label: &str,
    ctx: &RenderCtx,
) -> WindowResult {
    let icon = if ctx.config.nerd_font {
        format!("{icon_char} ")
    } else {
        format!("{label} ")
    };
    let bar_width = ctx.config.bar_width;
    let used_pct = rl.and_then(|w| w.used_percentage);

    let Some(used_pct) = used_pct else {
        let bar = build_bar(bar_width, None, None, ctx.config.nerd_font);
        let bar = colorize_bar(
            &bar,
            bar_width,
            None,
            ctx.config.nerd_font,
            ctx.theme,
            ctx.depth,
            Role::Dim,
        );
        let seg = format!(
            "{}{icon}{}[{bar}] n/a",
            ctx.theme.sgr(Role::Dim, ctx.depth),
            reset(ctx.depth)
        );
        return WindowResult {
            seg,
            used_pct: None,
            elapsed_pct: None,
            just_reset: false,
        };
    };

    let resets_at = rl.and_then(|w| w.resets_at);
    let (elapsed_pct, pace_idx, just_reset) = match resets_at {
        Some(resets_at) => {
            let elapsed_sec = total_seconds - (resets_at - ctx.now_sec);
            let e = (elapsed_sec / total_seconds * 100.0).clamp(0.0, 100.0);
            let idx = ((e / 100.0) * bar_width as f64).round() as usize;
            let just_reset = (0.0..JUST_RESET_WINDOW_SECONDS).contains(&elapsed_sec);
            (Some(e), Some(idx), just_reset)
        }
        None => (None, None, false),
    };

    let role = color_for_pace(Some(used_pct), elapsed_pct);
    let bar = build_bar(bar_width, Some(used_pct), pace_idx, ctx.config.nerd_font);
    let bar = colorize_bar(
        &bar,
        bar_width,
        pace_idx,
        ctx.config.nerd_font,
        ctx.theme,
        ctx.depth,
        role,
    );
    let pct = fmt_pct(Some(used_pct), ctx.config.percent_decimals);
    let seg = format!(
        "{}{icon}{}[{bar}] {pct}{}",
        ctx.theme.sgr(role, ctx.depth),
        reset(ctx.depth),
        reset(ctx.depth)
    );

    WindowResult {
        seg,
        used_pct: Some(used_pct),
        elapsed_pct,
        just_reset,
    }
}

/// Computes both rate-limit window segments. Always runs regardless of
/// whether `"5h"`/`"7d"` are in `Config::segments` — the pet and `reset`
/// segments need the raw numbers either way.
pub fn compute_windows(ctx: &RenderCtx) -> (WindowResult, WindowResult) {
    let five_h = window_segment(ctx.input.five_hour(), FIVE_HOUR_SECONDS, ICON_5H, "5h", ctx);
    let seven_d = window_segment(ctx.input.seven_day(), SEVEN_DAY_SECONDS, ICON_7D, "7d", ctx);
    (five_h, seven_d)
}

pub fn pet_segment(five_h: &WindowResult, ctx: &RenderCtx) -> String {
    if !ctx.config.pet_enabled {
        return String::new();
    }
    let cwd = ctx.input.resolved_cwd();
    let cwd_path = cwd.as_deref().map(std::path::Path::new);
    let inputs = PetInputs {
        ctx_pct: ctx.input.ctx_used_pct(),
        five_h_used: five_h.used_pct,
        five_h_elapsed: five_h.elapsed_pct,
        just_reset: five_h.just_reset,
        exceeds_200k: ctx.input.exceeds_200k_tokens.unwrap_or(false),
        now_sec: ctx.now_sec,
        session_id: ctx.input.session_id.as_deref(),
        cwd: cwd_path,
    };
    let (text, role) = pet::render(&inputs, ctx.config.nerd_font, ctx.config.pet_animate);
    colorize(&text, role, ctx)
}

// ---------------------------------------------------------------------------
// v2 segments (opt-in, plain text — no new icon glyphs)
// ---------------------------------------------------------------------------

fn cost_segment(ctx: &RenderCtx) -> Option<String> {
    let usd = ctx.input.cost.as_ref()?.total_cost_usd?;
    Some(colorize(&format!("${usd:.2}"), Role::Text, ctx))
}

fn duration_segment(ctx: &RenderCtx) -> Option<String> {
    let ms = ctx.input.cost.as_ref()?.total_duration_ms?;
    Some(colorize(
        &format!("dur {}", format_duration(ms / 1000.0)),
        Role::Text,
        ctx,
    ))
}

fn lines_segment(ctx: &RenderCtx) -> Option<String> {
    let cost = ctx.input.cost.as_ref()?;
    let added = cost.total_lines_added.unwrap_or(0);
    let removed = cost.total_lines_removed.unwrap_or(0);
    if cost.total_lines_added.is_none() && cost.total_lines_removed.is_none() {
        return None;
    }
    let text = format!("+{added}/-{removed}");
    let role = if added + removed == 0 {
        Role::Dim
    } else {
        Role::Text
    };
    Some(colorize(&text, role, ctx))
}

fn effort_segment(ctx: &RenderCtx) -> Option<String> {
    let level = ctx.input.effort.as_ref()?.level.as_deref()?;
    Some(colorize(&format!("eff {level}"), Role::Text, ctx))
}

fn mode_segment(ctx: &RenderCtx) -> Option<String> {
    let mut tags = Vec::new();
    if ctx
        .input
        .thinking
        .as_ref()
        .and_then(|t| t.enabled)
        .unwrap_or(false)
    {
        tags.push("thinking".to_string());
    }
    if ctx.input.fast_mode.unwrap_or(false) {
        tags.push("fast".to_string());
    }
    if let Some(name) = ctx
        .input
        .output_style
        .as_ref()
        .and_then(|o| o.name.as_deref())
    {
        if !name.eq_ignore_ascii_case("default") {
            tags.push(name.to_string());
        }
    }
    if let Some(name) = ctx.input.agent.as_ref().and_then(|a| a.name.as_deref()) {
        tags.push(format!("@{name}"));
    }
    if let Some(mode) = ctx.input.vim.as_ref().and_then(|v| v.mode.as_deref()) {
        tags.push(mode.to_lowercase());
    }
    if tags.is_empty() {
        return None;
    }
    Some(colorize(&tags.join(" "), Role::Accent, ctx))
}

fn pr_segment(ctx: &RenderCtx) -> Option<String> {
    let pr = ctx.input.pr.as_ref()?;
    let number = pr.number?;
    let mut text = format!("PR #{number}");
    let mut role = Role::Text;
    if let Some(state) = pr.review_state.as_deref() {
        let (label, r) = match state {
            "approved" => ("ok", Role::Ok),
            "pending" => ("pending", Role::Warn),
            "changes_requested" => ("changes", Role::Hot),
            "draft" => ("draft", Role::Dim),
            other => (other, Role::Text),
        };
        text.push_str(&format!(" {label}"));
        role = r;
    }
    Some(colorize(&text, role, ctx))
}

fn worktree_segment(ctx: &RenderCtx) -> Option<String> {
    let name = ctx.input.worktree_name()?;
    Some(colorize(&format!("wt {name}"), Role::Text, ctx))
}

fn repo_segment(ctx: &RenderCtx) -> Option<String> {
    let repo = ctx.input.workspace.as_ref()?.repo.as_ref()?;
    let owner = repo.owner.as_deref()?;
    let name = repo.name.as_deref()?;
    Some(colorize(&format!("{owner}/{name}"), Role::Dim, ctx))
}

fn reset_segment(ctx: &RenderCtx) -> Option<String> {
    let five_left = window_time_left(ctx.input.five_hour(), FIVE_HOUR_SECONDS, ctx.now_sec)
        .map(|s| format!("5h {}", format_duration(s)));
    let seven_left = window_time_left(ctx.input.seven_day(), SEVEN_DAY_SECONDS, ctx.now_sec)
        .map(|s| format!("7d {}", format_duration(s)));
    let parts: Vec<String> = [five_left, seven_left].into_iter().flatten().collect();
    if parts.is_empty() {
        return None;
    }
    Some(colorize(&parts.join(" "), Role::Dim, ctx))
}

fn window_time_left(rl: Option<&RateWindow>, total_seconds: f64, now_sec: f64) -> Option<f64> {
    let resets_at = rl?.resets_at?;
    Some((resets_at - now_sec).clamp(0.0, total_seconds))
}

fn tokens_segment(ctx: &RenderCtx) -> Option<String> {
    let cw = ctx.input.context_window.as_ref()?;
    let used = cw.total_input_tokens?;
    let size = cw.context_window_size.unwrap_or(200_000);
    Some(colorize(
        &format!("{}/{}", format_tokens(used), format_tokens(size)),
        Role::Text,
        ctx,
    ))
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        n.to_string()
    }
}

fn format_duration(seconds: f64) -> String {
    let secs = seconds.max(0.0) as i64;
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;
    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Builds every non-empty segment named in `Config::segments`, in that
/// order. `five_h`/`seven_d` are pre-computed by the caller (their raw
/// numbers feed the pet and `reset` segment even when those windows aren't
/// themselves in the segment list).
pub fn build_all(ctx: &RenderCtx, five_h: &WindowResult, seven_d: &WindowResult) -> Vec<Segment> {
    ctx.config
        .segments
        .iter()
        .filter_map(|name| {
            let text = match name.as_str() {
                "model" => Some(model_segment(ctx)),
                "git" => ctx.git.map(|g| git_segment(g, ctx)),
                "ctx" => Some(context_segment(ctx)),
                "5h" => Some(five_h.seg.clone()),
                "7d" => Some(seven_d.seg.clone()),
                "pet" => {
                    let seg = pet_segment(five_h, ctx);
                    if seg.is_empty() {
                        None
                    } else {
                        Some(seg)
                    }
                }
                "cost" => cost_segment(ctx),
                "duration" => duration_segment(ctx),
                "lines" => lines_segment(ctx),
                "effort" => effort_segment(ctx),
                "mode" => mode_segment(ctx),
                "pr" => pr_segment(ctx),
                "worktree" => worktree_segment(ctx),
                "repo" => repo_segment(ctx),
                "reset" => reset_segment(ctx),
                "tokens" => tokens_segment(ctx),
                _ => None,
            }?;
            Some(Segment {
                name: known_static_name(name),
                text,
                priority: priority_of(name),
            })
        })
        .collect()
}

/// `Segment::name` needs `'static` for cheap copying; segment names are
/// always drawn from `KNOWN_SEGMENTS`, so this is a lookup, not an alloc.
fn known_static_name(name: &str) -> &'static str {
    crate::config::KNOWN_SEGMENTS
        .iter()
        .find(|s| **s == name)
        .copied()
        .unwrap_or("segment")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_short_and_long() {
        assert_eq!(format_duration(90.0), "1m");
        assert_eq!(format_duration(3_661.0), "1h1m");
        assert_eq!(format_duration(90_000.0), "1d1h");
    }

    #[test]
    fn format_tokens_thresholds() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(84_000), "84k");
        assert_eq!(format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn priority_orders_core_segments_above_extras() {
        assert!(priority_of("model") > priority_of("cost"));
        assert!(priority_of("ctx") > priority_of("lines"));
    }
}
