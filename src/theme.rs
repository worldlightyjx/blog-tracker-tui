use std::collections::BTreeMap;

use ratatui::style::Color;
use serde::Deserialize;

const MIN_SELECTION_CONTRAST: f64 = 7.0;

/// Colors the user may override in `[theme]` of config.toml.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub selected_fg: Option<toml::Value>,
    pub selected_bg: Option<toml::Value>,
    pub unread_fg: Option<toml::Value>,
    pub read_fg: Option<toml::Value>,
    pub accent_fg: Option<toml::Value>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub selected_fg: Color,
    pub selected_bg: Color,
    pub unread_fg: Color,
    pub read_fg: Color,
    pub accent_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            selected_fg: Color::Rgb(255, 255, 255),
            selected_bg: Color::Rgb(30, 64, 175),
            unread_fg: Color::Rgb(250, 204, 21),
            read_fg: Color::Rgb(148, 163, 184),
            accent_fg: Color::Rgb(103, 232, 249),
        }
    }
}

impl ThemeConfig {
    /// Resolves overrides while retaining safe defaults for invalid values.
    /// Warnings are for display in the TUI; bad colors never stop the app.
    pub fn resolve(&self) -> (Theme, Vec<String>) {
        let mut theme = Theme::default();
        let mut warnings = Vec::new();

        match (&self.selected_fg, &self.selected_bg) {
            (None, None) => {}
            (Some(fg), Some(bg)) => {
                let fg = parse_override("selected_fg", fg, &mut warnings);
                let bg = parse_override("selected_bg", bg, &mut warnings);
                if let (Some(fg), Some(bg)) = (fg, bg) {
                    let ratio = contrast_ratio(fg, bg).expect("parsed colors are RGB");
                    if ratio >= MIN_SELECTION_CONTRAST {
                        theme.selected_fg = fg;
                        theme.selected_bg = bg;
                    } else {
                        warnings.push(format!(
                            "selected_fg/selected_bg contrast is {ratio:.1}:1; both defaults used (minimum 7:1)"
                        ));
                    }
                } else {
                    warnings.push("selected_fg/selected_bg defaults used together".into());
                }
            }
            _ => {
                warnings.push("set selected_fg and selected_bg together; both defaults used".into())
            }
        }

        for (name, value, slot) in [
            ("unread_fg", &self.unread_fg, &mut theme.unread_fg),
            ("read_fg", &self.read_fg, &mut theme.read_fg),
            ("accent_fg", &self.accent_fg, &mut theme.accent_fg),
        ] {
            if let Some(value) = value
                && let Some(color) = parse_override(name, value, &mut warnings)
            {
                *slot = color;
            }
        }
        for key in self.unknown.keys() {
            warnings.push(format!("unknown theme key '{key}' ignored"));
        }
        (theme, warnings)
    }
}

fn parse_override(name: &str, value: &toml::Value, warnings: &mut Vec<String>) -> Option<Color> {
    let Some(raw) = value.as_str() else {
        warnings.push(format!(
            "{name} must be a quoted #RRGGBB value; default used"
        ));
        return None;
    };
    let Some(hex) = raw.strip_prefix('#') else {
        warnings.push(format!("{name} must be #RRGGBB; default used"));
        return None;
    };
    if hex.len() != 6 || !hex.is_ascii() {
        warnings.push(format!("{name} must be #RRGGBB; default used"));
        return None;
    }
    let parsed = (|| {
        Some(Color::Rgb(
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ))
    })();
    if parsed.is_none() {
        warnings.push(format!("{name} must be #RRGGBB; default used"));
    }
    parsed
}

fn luminance(red: u8, green: u8, blue: u8) -> f64 {
    let linear = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
}

/// Contrast ratio of two RGB colors, using relative luminance.
pub fn contrast_ratio(foreground: Color, background: Color) -> Option<f64> {
    let (Color::Rgb(fr, fg, fb), Color::Rgb(br, bg, bb)) = (foreground, background) else {
        return None;
    };
    let a = luminance(fr, fg, fb);
    let b = luminance(br, bg, bb);
    Some((a.max(b) + 0.05) / (a.min(b) + 0.05))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_toml(raw: &str) -> ThemeConfig {
        toml::from_str(raw).unwrap()
    }

    #[test]
    fn resolves_valid_custom_colors() {
        let config = from_toml(
            "selected_fg = '#ffffff'\nselected_bg = '#000000'\nunread_fg = '#facc15'\nread_fg = '#94a3b8'\naccent_fg = '#67e8f9'",
        );
        let (theme, warnings) = config.resolve();
        assert!(warnings.is_empty());
        assert_eq!(theme.selected_bg, Color::Rgb(0, 0, 0));
        assert_eq!(theme.unread_fg, Color::Rgb(250, 204, 21));
        assert_eq!(theme.read_fg, Color::Rgb(148, 163, 184));
        assert_eq!(theme.accent_fg, Color::Rgb(103, 232, 249));
    }

    #[test]
    fn low_contrast_or_incomplete_selection_uses_safe_pair() {
        for raw in [
            "selected_fg = '#ffffff'\nselected_bg = '#ffffff'",
            "selected_fg = '#ffffff'",
            "selected_fg = '#xxxxxx'\nselected_bg = '#000000'",
        ] {
            let (theme, warnings) = from_toml(raw).resolve();
            assert_eq!(theme.selected_fg, Theme::default().selected_fg);
            assert_eq!(theme.selected_bg, Theme::default().selected_bg);
            assert!(!warnings.is_empty());
        }
    }

    #[test]
    fn invalid_or_unknown_colors_warn_without_blocking_other_overrides() {
        let (theme, warnings) =
            from_toml("unread_fg = 42\nread_fg = '#334455'\naccent_colour = '#ffffff'").resolve();
        assert_eq!(theme.unread_fg, Theme::default().unread_fg);
        assert_eq!(theme.read_fg, Color::Rgb(51, 68, 85));
        assert_eq!(warnings.len(), 2);
    }
}
