//! Truecolor palette engine: RGB colors, terminal-capability detection, and
//! named theme presets. Themes are always authored in 24-bit RGB; rendering
//! quantizes down to whatever the terminal actually supports (256-color,
//! basic 16-color ANSI, or no color at all) so callers never need to think
//! about depth — they ask for a `Role` and get back an SGR-ready string for
//! the terminal they're actually running in.

pub const RESET: &str = "\x1b[0m";

/// `RESET`, or the empty string at `ColorDepth::None` — callers that build up
/// colored spans must use this instead of the bare constant, since emitting
/// a reset code with no matching color-start would still put escape bytes
/// into a line that's supposed to be plain (`NO_COLOR`, `TERM=dumb`, etc.).
pub fn reset(depth: ColorDepth) -> &'static str {
    if depth == ColorDepth::None {
        ""
    } else {
        RESET
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Ansi256,
    Ansi16,
    None,
}

/// Which threshold band / UI role a piece of text plays. `Ok`/`Warn`/`Hot`
/// mirror the pace/context thresholds; `Dim` is for separators and "n/a",
/// `Text` is default foreground, `Accent` is the pace marker and highlights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Ok,
    Warn,
    Hot,
    Dim,
    Text,
    Accent,
}

/// Detects how many colors the terminal supports. Precedence: `NO_COLOR` (any
/// value, per the no-color.org convention) forces `None`;
/// `CLAUDE_STATUSLINE_COLOR` (`auto|truecolor|256|16|none`) is an explicit
/// override; then `COLORTERM=truecolor|24bit`; then `TERM` containing
/// `256color`; then `TERM=dumb`; then platform default (Windows Terminal and
/// modern conhost both support truecolor even when `COLORTERM` isn't set, so
/// Windows defaults to `TrueColor`; elsewhere we default to the safe `Ansi16`
/// since plain xterm doesn't reliably advertise more).
pub fn detect_depth() -> ColorDepth {
    if std::env::var_os("NO_COLOR").is_some() {
        return ColorDepth::None;
    }
    if let Ok(v) = std::env::var("CLAUDE_STATUSLINE_COLOR") {
        match v.as_str() {
            "truecolor" => return ColorDepth::TrueColor,
            "256" => return ColorDepth::Ansi256,
            "16" => return ColorDepth::Ansi16,
            "none" => return ColorDepth::None,
            _ => {} // "auto" or unrecognized: fall through to detection
        }
    }
    if let Ok(v) = std::env::var("COLORTERM") {
        if v == "truecolor" || v == "24bit" {
            return ColorDepth::TrueColor;
        }
    }
    if let Ok(term) = std::env::var("TERM") {
        if term == "dumb" {
            return ColorDepth::None;
        }
        if term.contains("256color") {
            return ColorDepth::Ansi256;
        }
    }
    if cfg!(target_os = "windows") {
        ColorDepth::TrueColor
    } else {
        ColorDepth::Ansi16
    }
}

const CUBE_STEPS: [u16; 6] = [0, 95, 135, 175, 215, 255];

fn nearest_cube_idx(v: u8) -> u8 {
    CUBE_STEPS
        .iter()
        .enumerate()
        .min_by_key(|(_, &step)| (step as i32 - v as i32).abs())
        .map(|(i, _)| i as u8)
        .unwrap_or(0)
}

/// The 16 basic ANSI colors as approximate RGB, paired with their SGR
/// foreground codes, for nearest-color matching.
const ANSI16: [(u8, Rgb); 16] = [
    (30, Rgb(0, 0, 0)),
    (31, Rgb(128, 0, 0)),
    (32, Rgb(0, 128, 0)),
    (33, Rgb(128, 128, 0)),
    (34, Rgb(0, 0, 128)),
    (35, Rgb(128, 0, 128)),
    (36, Rgb(0, 128, 128)),
    (37, Rgb(192, 192, 192)),
    (90, Rgb(128, 128, 128)),
    (91, Rgb(255, 0, 0)),
    (92, Rgb(0, 255, 0)),
    (93, Rgb(255, 255, 0)),
    (94, Rgb(0, 0, 255)),
    (95, Rgb(255, 0, 255)),
    (96, Rgb(0, 255, 255)),
    (97, Rgb(255, 255, 255)),
];

impl Rgb {
    fn dist_sq(&self, other: &Rgb) -> i32 {
        let dr = self.0 as i32 - other.0 as i32;
        let dg = self.1 as i32 - other.1 as i32;
        let db = self.2 as i32 - other.2 as i32;
        dr * dr + dg * dg + db * db
    }

    fn to_ansi256(self) -> u8 {
        16 + 36 * nearest_cube_idx(self.0) + 6 * nearest_cube_idx(self.1) + nearest_cube_idx(self.2)
    }

    fn to_ansi16_code(self) -> u8 {
        ANSI16
            .iter()
            .min_by_key(|(_, rgb)| self.dist_sq(rgb))
            .map(|(code, _)| *code)
            .unwrap_or(37)
    }

    /// Renders this color as an SGR foreground escape for `depth`. Empty
    /// string for `ColorDepth::None` — callers can unconditionally splice
    /// this in without a separate "did we emit color" branch.
    pub fn to_sgr(self, depth: ColorDepth) -> String {
        match depth {
            ColorDepth::TrueColor => format!("\x1b[38;2;{};{};{}m", self.0, self.1, self.2),
            ColorDepth::Ansi256 => format!("\x1b[38;5;{}m", self.to_ansi256()),
            ColorDepth::Ansi16 => format!("\x1b[{}m", self.to_ansi16_code()),
            ColorDepth::None => String::new(),
        }
    }

    fn lerp(a: Rgb, b: Rgb, t: f64) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let l = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
        Rgb(l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
    }
}

/// Ordered ok -> warn -> hot color stops used both for bar-cell gradient
/// tinting (position-based) and threshold coloring (value-based).
#[derive(Debug, Clone, Copy)]
pub struct Ramp {
    pub ok: Rgb,
    pub warn: Rgb,
    pub hot: Rgb,
}

impl Ramp {
    /// Linearly interpolates along the ramp at `t` in `[0.0, 1.0]`: `0.0` is
    /// pure `ok`, `0.5` is pure `warn`, `1.0` is pure `hot`.
    pub fn at(&self, t: f64) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        if t <= 0.5 {
            Rgb::lerp(self.ok, self.warn, t / 0.5)
        } else {
            Rgb::lerp(self.warn, self.hot, (t - 0.5) / 0.5)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub ramp: Ramp,
    pub dim: Rgb,
    pub text: Rgb,
    pub accent: Rgb,
}

impl Theme {
    pub fn by_name(name: &str) -> Theme {
        match name {
            "warm" => Theme::warm(),
            "mono" => Theme::mono(),
            "dracula" => Theme::dracula(),
            "nord" => Theme::nord(),
            _ => Theme::neon(), // "neon" and any unrecognized name
        }
    }

    pub fn role_rgb(&self, role: Role) -> Rgb {
        match role {
            Role::Ok => self.ramp.ok,
            Role::Warn => self.ramp.warn,
            Role::Hot => self.ramp.hot,
            Role::Dim => self.dim,
            Role::Text => self.text,
            Role::Accent => self.accent,
        }
    }

    /// SGR for `role` at the given color depth; empty string when `depth` is
    /// `None`.
    pub fn sgr(&self, role: Role, depth: ColorDepth) -> String {
        self.role_rgb(role).to_sgr(depth)
    }

    fn neon() -> Theme {
        Theme {
            ramp: Ramp {
                ok: Rgb(57, 255, 20),
                warn: Rgb(255, 214, 10),
                hot: Rgb(255, 45, 85),
            },
            dim: Rgb(90, 90, 110),
            text: Rgb(230, 230, 255),
            accent: Rgb(0, 229, 255),
        }
    }

    fn warm() -> Theme {
        Theme {
            ramp: Ramp {
                ok: Rgb(154, 205, 50),
                warn: Rgb(255, 165, 0),
                hot: Rgb(220, 20, 60),
            },
            dim: Rgb(120, 100, 90),
            text: Rgb(255, 235, 205),
            accent: Rgb(255, 140, 0),
        }
    }

    fn mono() -> Theme {
        Theme {
            ramp: Ramp {
                ok: Rgb(180, 180, 180),
                warn: Rgb(140, 140, 140),
                hot: Rgb(255, 255, 255),
            },
            dim: Rgb(90, 90, 90),
            text: Rgb(220, 220, 220),
            accent: Rgb(255, 255, 255),
        }
    }

    fn dracula() -> Theme {
        Theme {
            ramp: Ramp {
                ok: Rgb(80, 250, 123),
                warn: Rgb(241, 250, 140),
                hot: Rgb(255, 85, 85),
            },
            dim: Rgb(98, 114, 164),
            text: Rgb(248, 248, 242),
            accent: Rgb(189, 147, 249),
        }
    }

    fn nord() -> Theme {
        Theme {
            ramp: Ramp {
                ok: Rgb(163, 190, 140),
                warn: Rgb(235, 203, 139),
                hot: Rgb(191, 97, 106),
            },
            dim: Rgb(76, 86, 106),
            text: Rgb(216, 222, 233),
            accent: Rgb(136, 192, 208),
        }
    }
}

pub const THEME_NAMES: [&str; 5] = ["neon", "warm", "mono", "dracula", "nord"];

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env-var mutation in `detect_depth` tests must not race other tests in
    // this process.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_color_env() {
        for k in ["NO_COLOR", "CLAUDE_STATUSLINE_COLOR", "COLORTERM", "TERM"] {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn no_color_wins_over_everything() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_color_env();
        std::env::set_var("NO_COLOR", "1");
        std::env::set_var("COLORTERM", "truecolor");
        assert_eq!(detect_depth(), ColorDepth::None);
        clear_color_env();
    }

    #[test]
    fn colorterm_truecolor_detected() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_color_env();
        std::env::set_var("COLORTERM", "truecolor");
        assert_eq!(detect_depth(), ColorDepth::TrueColor);
        clear_color_env();
    }

    #[test]
    fn term_256color_detected() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_color_env();
        std::env::set_var("TERM", "xterm-256color");
        assert_eq!(detect_depth(), ColorDepth::Ansi256);
        clear_color_env();
    }

    #[test]
    fn explicit_override_wins_over_colorterm() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_color_env();
        std::env::set_var("COLORTERM", "truecolor");
        std::env::set_var("CLAUDE_STATUSLINE_COLOR", "16");
        assert_eq!(detect_depth(), ColorDepth::Ansi16);
        clear_color_env();
    }

    #[test]
    fn none_depth_emits_no_escape() {
        let rgb = Rgb(255, 0, 0);
        assert_eq!(rgb.to_sgr(ColorDepth::None), "");
    }

    #[test]
    fn truecolor_emits_24bit_escape() {
        let rgb = Rgb(1, 2, 3);
        assert_eq!(rgb.to_sgr(ColorDepth::TrueColor), "\x1b[38;2;1;2;3m");
    }

    #[test]
    fn ansi256_quantizes_pure_red() {
        // Pure red (255,0,0) should land on cube index (5,0,0) -> 16+36*5=196.
        assert_eq!(Rgb(255, 0, 0).to_ansi256(), 196);
    }

    #[test]
    fn ansi16_quantization_is_stable() {
        // Same input always resolves to the same code.
        let a = Rgb(10, 200, 10).to_ansi16_code();
        let b = Rgb(10, 200, 10).to_ansi16_code();
        assert_eq!(a, b);
        assert_eq!(a, 92); // nearest to bright green
    }

    #[test]
    fn ramp_at_endpoints_and_midpoint() {
        let ramp = Ramp {
            ok: Rgb(0, 255, 0),
            warn: Rgb(255, 255, 0),
            hot: Rgb(255, 0, 0),
        };
        assert_eq!(ramp.at(0.0), Rgb(0, 255, 0));
        assert_eq!(ramp.at(0.5), Rgb(255, 255, 0));
        assert_eq!(ramp.at(1.0), Rgb(255, 0, 0));
    }

    #[test]
    fn reset_is_empty_at_none_depth() {
        assert_eq!(reset(ColorDepth::None), "");
        assert_eq!(reset(ColorDepth::TrueColor), RESET);
    }

    #[test]
    fn all_theme_names_resolve() {
        for name in THEME_NAMES {
            let _ = Theme::by_name(name);
        }
        // Unknown name falls back to neon rather than panicking.
        let _ = Theme::by_name("does-not-exist");
    }
}
