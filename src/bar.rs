//! Bar rendering, pace-marker placement, and threshold coloring.
//!
//! `build_bar` produces plain (uncolored) bar text; `colorize_bar` applies
//! theme coloring on top of it. Splitting the two keeps `build_bar`'s output
//! (and its tests) stable regardless of theme/color-depth — only the ANSI
//! wrapped around it changes.

use crate::theme::{ColorDepth, Rgb, Role, Theme, RESET};

pub const BAR_WIDTH: usize = 10;

pub fn clamp(n: f64, lo: f64, hi: f64) -> f64 {
    n.max(lo).min(hi)
}

/// Eighth-block glyphs for 1/8 through 7/8 fill, in Unicode "Block Elements"
/// order (U+258F down to U+2589 — each one eighth more filled than the last).
const EIGHTHS: [char; 7] = [
    '\u{258F}', '\u{258E}', '\u{258D}', '\u{258C}', '\u{258B}', '\u{258A}', '\u{2589}',
];

/// Renders a `width`-character bar. `used_pct` fills from the left; in nerd
/// (Unicode) mode the fill is sub-cell accurate to 1/8 of a cell (80 steps at
/// width 10, vs. 10 whole-cell steps in ASCII mode). `pace_idx`, if given,
/// overlays a `|` at that cell — clamped into range — regardless of fill
/// state, so it's always visible. Returns plain text; see `colorize_bar` for
/// applying theme colors.
pub fn build_bar(
    width: usize,
    used_pct: Option<f64>,
    pace_idx: Option<usize>,
    nerd: bool,
) -> String {
    let empty_char = if nerd { '\u{2591}' } else { '-' };
    let full_char = if nerd { '\u{2588}' } else { '#' };

    let mut chars: Vec<char> = match used_pct {
        None => vec![empty_char; width],
        Some(p) if nerd => build_eighths(width, p, empty_char, full_char),
        Some(p) => {
            let filled = (clamp(p, 0.0, 100.0) / 100.0 * width as f64).round() as usize;
            (0..width)
                .map(|i| if i < filled { full_char } else { empty_char })
                .collect()
        }
    };

    if let Some(idx) = pace_idx {
        let idx = idx.min(width.saturating_sub(1));
        chars[idx] = '|';
    }
    chars.into_iter().collect()
}

fn build_eighths(width: usize, pct: f64, empty_char: char, full_char: char) -> Vec<char> {
    let total_eighths = (clamp(pct, 0.0, 100.0) / 100.0 * width as f64 * 8.0).round() as usize;
    let total_eighths = total_eighths.min(width * 8);
    let full_cells = total_eighths / 8;
    let remainder = total_eighths % 8;

    (0..width)
        .map(|i| {
            if i < full_cells {
                full_char
            } else if i == full_cells && remainder > 0 {
                EIGHTHS[remainder - 1]
            } else {
                empty_char
            }
        })
        .collect()
}

/// Applies theme coloring to a plain bar built by `build_bar`.
///
/// ASCII mode (`nerd = false`): the whole bar takes one flat color from
/// `threshold_role`, matching v1's behavior exactly.
///
/// Nerd/Unicode mode: each cell is tinted along the theme's ramp by its
/// *position* (`i / width`), so the leading edge of every bar is always cool
/// and the tail is always hot — independent of how full the bar actually is.
/// The pace-marker cell is colored with the theme's accent instead, so it
/// stays visible against the tint. Consecutive cells sharing a color are
/// coalesced into one escape rather than one per cell.
pub fn colorize_bar(
    bar: &str,
    width: usize,
    pace_idx: Option<usize>,
    nerd: bool,
    theme: &Theme,
    depth: ColorDepth,
    threshold_role: Role,
) -> String {
    if depth == ColorDepth::None {
        return bar.to_string();
    }
    if !nerd {
        let sgr = theme.sgr(threshold_role, depth);
        return format!("{sgr}{bar}{RESET}");
    }

    let pace_idx = pace_idx.map(|i| i.min(width.saturating_sub(1)));
    let mut out = String::new();
    let mut last: Option<Rgb> = None;
    for (i, ch) in bar.chars().enumerate() {
        let rgb = if Some(i) == pace_idx {
            theme.role_rgb(Role::Accent)
        } else {
            let t = if width == 0 {
                0.0
            } else {
                i as f64 / width as f64
            };
            theme.ramp.at(t)
        };
        if last != Some(rgb) {
            out.push_str(&rgb.to_sgr(depth));
            last = Some(rgb);
        }
        out.push(ch);
    }
    out.push_str(RESET);
    out
}

/// Pace-relative role for the 5h/7d rate-limit bars: `Ok` if usage is at or
/// under the pace the clock has set (`used% <= elapsed%`), `Warn` up to 15
/// points ahead of pace, `Hot` beyond that. `Dim` ("n/a") when either input
/// is missing.
pub fn color_for_pace(used_pct: Option<f64>, elapsed_pct: Option<f64>) -> Role {
    match (used_pct, elapsed_pct) {
        (Some(u), Some(e)) => {
            let diff = u - e;
            if diff <= 0.0 {
                Role::Ok
            } else if diff <= 15.0 {
                Role::Warn
            } else {
                Role::Hot
            }
        }
        _ => Role::Dim,
    }
}

/// Flat threshold role for the context-window bar (no pace concept): `Ok`
/// under 50%, `Warn` 50-80%, `Hot` above 80%.
pub fn color_for_context(pct: Option<f64>) -> Role {
    match pct {
        None => Role::Dim,
        Some(p) if p < 50.0 => Role::Ok,
        Some(p) if p <= 80.0 => Role::Warn,
        _ => Role::Hot,
    }
}

pub fn fmt_pct(pct: Option<f64>, decimals: u8) -> String {
    match pct {
        None => "n/a".to_string(),
        Some(p) => format!("{:.*}%", decimals as usize, p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn bar_fills_and_places_marker() {
        // used=60%, pace at index 5 -> 5 filled, marker at 5, then dashes.
        assert_eq!(build_bar(10, Some(60.0), Some(5), false), "#####|----");
        // used=20%, pace at index 5 -> 2 filled, gap, marker, gap.
        assert_eq!(build_bar(10, Some(20.0), Some(5), false), "##---|----");
    }

    #[test]
    fn bar_with_no_data_is_all_empty() {
        assert_eq!(build_bar(10, None, None, false), "----------");
    }

    #[test]
    fn pace_index_clamped_into_range() {
        assert_eq!(build_bar(10, Some(0.0), Some(999), false), "---------|");
    }

    #[test]
    fn pace_colors() {
        assert_eq!(color_for_pace(Some(60.0), Some(50.0)), Role::Warn); // 10 ahead
        assert_eq!(color_for_pace(Some(20.0), Some(50.0)), Role::Ok); // behind pace
        assert_eq!(color_for_pace(Some(95.0), Some(11.0)), Role::Hot); // way ahead
        assert_eq!(color_for_pace(None, Some(50.0)), Role::Dim);
    }

    #[test]
    fn context_colors() {
        assert_eq!(color_for_context(Some(30.0)), Role::Ok);
        assert_eq!(color_for_context(Some(50.0)), Role::Warn);
        assert_eq!(color_for_context(Some(80.0)), Role::Warn);
        assert_eq!(color_for_context(Some(81.0)), Role::Hot);
        assert_eq!(color_for_context(None), Role::Dim);
    }

    #[test]
    fn fmt_pct_rounds_and_handles_none() {
        assert_eq!(fmt_pct(Some(41.6), 0), "42%");
        assert_eq!(fmt_pct(None, 0), "n/a");
    }

    #[test]
    fn fmt_pct_respects_decimals() {
        assert_eq!(fmt_pct(Some(41.6), 1), "41.6%");
    }

    #[test]
    fn eighths_sub_cell_precision() {
        // 5% of a 10-wide bar = 0.5 cell -> half-eighths -> 4/8 = left-half block.
        let bar = build_bar(10, Some(5.0), None, true);
        assert_eq!(bar.chars().next().unwrap(), '\u{258C}');
        // 100% fills every cell fully.
        let full = build_bar(10, Some(100.0), None, true);
        assert!(full.chars().all(|c| c == '\u{2588}'));
        // 0% is all empty.
        let empty = build_bar(10, Some(0.0), None, true);
        assert!(empty.chars().all(|c| c == '\u{2591}'));
    }

    #[test]
    fn ascii_mode_ignores_sub_cell_precision() {
        // ASCII mode always rounds to whole cells regardless of nerd-mode
        // math: 5% of a 10-wide bar rounds to the nearest whole cell (1),
        // not a fractional glyph.
        assert_eq!(build_bar(10, Some(5.0), None, false), "#---------");
    }

    #[test]
    fn colorize_ascii_mode_wraps_whole_bar_one_color() {
        let theme = Theme::by_name("mono");
        let bar = build_bar(10, Some(60.0), None, false);
        let colored = colorize_bar(
            &bar,
            10,
            None,
            false,
            &theme,
            ColorDepth::TrueColor,
            Role::Ok,
        );
        // Exactly one color escape (plus the trailing reset).
        assert_eq!(colored.matches('\x1b').count(), 2);
    }

    #[test]
    fn colorize_none_depth_is_plain() {
        let theme = Theme::by_name("neon");
        let bar = build_bar(10, Some(60.0), Some(5), true);
        let colored = colorize_bar(&bar, 10, Some(5), true, &theme, ColorDepth::None, Role::Ok);
        assert_eq!(colored, bar);
    }
}
