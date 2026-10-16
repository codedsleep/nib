//! GTK named-color parsing and palette transfer, following Trek's color behavior.
#![allow(clippy::float_cmp)] // Exact comparisons identify literal palette channels.
use std::{
    collections::{BTreeMap, HashSet},
    fmt::Write,
};
#[derive(Debug, Clone, Copy, PartialEq)]
struct Color {
    rgb: [f64; 3],
    alpha: f64,
}
impl Color {
    fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let opaque = |rgb| Self { rgb, alpha: 1.0 };
        match s {
            "black" => return Some(opaque([0.0; 3])),
            "white" => return Some(opaque([255.0; 3])),
            _ => {}
        }
        if let Some(h) = s.strip_prefix('#') {
            if ![3, 4, 6, 8].contains(&h.len()) || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let bytes = if h.len() <= 4 {
                h.chars()
                    .map(|c| c.to_digit(16).map(|n| f64::from(n) * 17.0))
                    .collect::<Option<Vec<_>>>()?
            } else {
                h.as_bytes()
                    .chunks(2)
                    .map(|p| {
                        std::str::from_utf8(p)
                            .ok()
                            .and_then(|s| u8::from_str_radix(s, 16).ok())
                            .map(f64::from)
                    })
                    .collect::<Option<Vec<_>>>()?
            };
            return Some(Self {
                rgb: [bytes[0], bytes[1], bytes[2]],
                alpha: bytes.get(3).map_or(1.0, |a| a / 255.0),
            });
        }
        let v = s
            .strip_prefix("rgba(")
            .or_else(|| s.strip_prefix("rgb("))?
            .strip_suffix(')')?
            .split(',')
            .map(|s| s.trim().parse::<f64>().ok())
            .collect::<Option<Vec<_>>>()?;
        if !(3..=4).contains(&v.len()) || v.iter().any(|n| !n.is_finite()) {
            return None;
        }
        Some(Self {
            rgb: [v[0], v[1], v[2]],
            alpha: v.get(3).copied().unwrap_or(1.0),
        })
    }
    fn min(self) -> f64 {
        self.rgb.into_iter().fold(255.0, f64::min)
    }
    fn chroma(self) -> f64 {
        self.rgb.into_iter().fold(0.0, f64::max) - self.min()
    }
    fn level(self) -> f64 {
        self.rgb.iter().sum::<f64>() / 3.0
    }
    #[allow(clippy::many_single_char_names)] // Conventional RGB/HSV coordinates.
    fn hue(self) -> f64 {
        let [r, g, b] = self.rgb;
        let c = self.chroma();
        if c == 0.0 {
            return 0.0;
        }
        let m = self.min() + c;
        let h = if m == r {
            ((g - b) / c).rem_euclid(6.0)
        } else if m == g {
            (b - r) / c + 2.0
        } else {
            (r - g) / c + 4.0
        };
        h * 60.0
    }
    fn mix(self, b: Self, t: f64) -> Self {
        Self {
            rgb: std::array::from_fn(|i| self.rgb[i] + (b.rgb[i] - self.rgb[i]) * t),
            alpha: 1.0,
        }
    }
    fn over(self, b: Self) -> Self {
        Self {
            rgb: std::array::from_fn(|i| self.rgb[i] * self.alpha + b.rgb[i] * (1.0 - self.alpha)),
            alpha: 1.0,
        }
    }
    fn css(self) -> String {
        let [r, g, b] = self
            .rgb
            .map(|v| format!("{:.0}", v.round().clamp(0.0, 255.0)));
        if (self.alpha - 1.0).abs() < 1e-9 {
            format!("rgb({r}, {g}, {b})")
        } else {
            format!("rgba({r}, {g}, {b}, {})", self.alpha)
        }
    }
}
/// Named GTK colors; references are resolved lazily with cycle detection.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    values: BTreeMap<String, String>,
}
impl Palette {
    fn get(&self, name: &str) -> Option<Color> {
        let mut name = name;
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(name) {
                return None;
            }
            let value = self.values.get(name)?.trim();
            if let Some(next) = value.strip_prefix('@') {
                name = next;
            } else {
                return Color::parse(value);
            }
        }
    }
    /// True for a palette whose background luminance is below halfway.
    pub fn is_dark(&self) -> Option<bool> {
        self.get("theme_bg_color").map(|c| c.level() < 128.0)
    }
    /// Whether any color definitions were found.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    /// Resolved color as a CSS literal, useful to frontend diagnostics.
    pub fn color(&self, name: &str) -> Option<String> {
        self.get(name).map(Color::css)
    }
}
/// Read `@define-color` statements; the last definition wins.
pub fn parse_gtk3_colors(css: &str) -> Palette {
    let mut p = Palette::default();
    let mut rest = css;
    while let Some(i) = rest.find("@define-color") {
        rest = &rest[i + 13..];
        let Some(end) = rest.find(';') else {
            break;
        };
        if let Some((name, value)) = rest[..end].trim().split_once(char::is_whitespace) {
            p.values.insert(name.into(), value.trim().into());
        }
        rest = &rest[end + 1..];
    }
    p
}
struct Mapping {
    exact: Vec<(Color, Color)>,
    greys: Vec<(f64, Color)>,
    families: Vec<(Color, Color)>,
    selected: Color,
    accent: Color,
}
impl Mapping {
    fn new(base: &Palette, p: &Palette) -> Option<Self> {
        let bg = p.get("theme_bg_color")?;
        let solid = |name| p.get(name).map(|c| c.over(bg));
        let fg = solid("theme_fg_color")?;
        let accent = base.get("theme_selected_bg_color")?;
        p.get("theme_selected_bg_color")?;
        let mut greys = vec![
            (
                0.0,
                Color {
                    rgb: [0.0; 3],
                    alpha: 1.0,
                },
            ),
            (
                255.0,
                Color {
                    rgb: [255.0; 3],
                    alpha: 1.0,
                },
            ),
        ];
        for (name, to) in [
            ("theme_fg_color", fg),
            ("theme_text_color", solid("theme_text_color").unwrap_or(fg)),
            ("theme_bg_color", bg),
            ("theme_base_color", solid("theme_base_color").unwrap_or(bg)),
        ] {
            if let Some(from) = base.get(name) {
                greys.push((from.level(), to));
            }
        }
        greys.sort_by(|a, b| a.0.total_cmp(&b.0));
        greys.dedup_by(|a, b| a.0 == b.0);
        let exact = base
            .values
            .keys()
            .filter_map(|n| Some((base.get(n)?, p.get(n)?)))
            .filter(|(f, _)| f.level() > 0.0 && f.level() < 255.0)
            .collect();
        let families = [
            "theme_selected_bg_color",
            "error_color",
            "success_color",
            "warning_color",
        ]
        .iter()
        .filter_map(|n| Some((base.get(n)?, solid(n)?)))
        .filter(|(f, _)| f.chroma() >= 12.0)
        .collect();
        Some(Self {
            exact,
            greys,
            families,
            selected: p
                .get("theme_selected_fg_color")
                .or_else(|| base.get("theme_selected_fg_color"))
                .unwrap_or(Color {
                    rgb: [255.0; 3],
                    alpha: 1.0,
                }),
            accent,
        })
    }
    fn near(a: Color, b: Color) -> bool {
        let gap = (a.hue() - b.hue()).abs();
        gap.min(360.0 - gap) <= 20.0
    }
    fn map(&self, c: Color) -> Color {
        if let Some((_, to)) = self.exact.iter().find(|(f, _)| f.rgb == c.rgb) {
            return Color {
                alpha: to.alpha * c.alpha,
                ..*to
            };
        }
        if c.chroma() < 12.0 {
            let level = c.level();
            let hi = self
                .greys
                .partition_point(|(l, _)| *l < level)
                .min(self.greys.len() - 1);
            let lo = hi.saturating_sub(1);
            let (a, from) = self.greys[lo];
            let (b, to) = self.greys[hi];
            let mixed = from.mix(to, if a == b { 0.0 } else { (level - a) / (b - a) });
            return Color {
                alpha: c.alpha,
                ..mixed
            };
        }
        if let Some((from, to)) = self.families.iter().find(|(f, _)| Self::near(c, *f)) {
            let scale = c.chroma() / from.chroma();
            let gray = c.min() - scale * from.min();
            return Color {
                rgb: to.rgb.map(|v| (v * scale + gray).clamp(0.0, 255.0)),
                alpha: c.alpha,
            };
        }
        c
    }
}
fn token_at(text: &str, i: usize) -> Option<(usize, Color, bool)> {
    let s = text.get(i..)?;
    let b = *text.as_bytes().get(i)?;
    let boundary = i == 0 || !text.as_bytes()[i - 1].is_ascii_alphanumeric();
    let end = if b == b'#' {
        i + 1 + s[1..].bytes().take_while(u8::is_ascii_hexdigit).count()
    } else if boundary && (s.starts_with("rgb(") || s.starts_with("rgba(")) {
        i + s.find(')')? + 1
    } else if boundary && (s.starts_with("white") || s.starts_with("black")) {
        i + 5
    } else {
        return None;
    };
    if text
        .as_bytes()
        .get(end)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
    {
        return None;
    }
    Some((end, Color::parse(&text[i..end])?, matches!(b, b'w' | b'b')))
}
fn rewrite_value(value: &str, m: &Mapping, property: &str, on_accent: bool) -> String {
    let mut out = String::new();
    let mut i = 0;
    let mut quote = None;
    while i < value.len() {
        let c = value[i..].chars().next().expect("valid boundary");
        if matches!(c, '\'' | '"') {
            if quote == Some(c) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(c);
            }
        }
        if quote.is_none()
            && let Some((end, color, word)) = token_at(value, i)
        {
            let mapped =
                if property.trim() == "color" && color.rgb == [255.0; 3] && (!word || on_accent) {
                    m.selected
                } else {
                    m.map(color)
                };
            out.push_str(&mapped.css());
            i = end;
            continue;
        }
        out.push(c);
        i += c.len_utf8();
    }
    out
}
/// Recolor declaration literals and export theme names, leaving selectors and
/// quoted URLs intact. Missing essential colors leave the stylesheet unchanged.
pub fn recolour(css: &str, palette: &Palette) -> String {
    let Some(m) = Mapping::new(&parse_gtk3_colors(css), palette) else {
        return css.into();
    };
    let mut out = String::new();
    let mut start = 0;
    let mut depth = 0;
    let mut quote = None;
    let mut comment = false;
    // Split only at structural CSS boundaries; colors never occur in selectors.
    for (i, c) in css.char_indices() {
        if comment {
            if i > 0 && c == '/' && css.as_bytes()[i - 1] == b'*' {
                comment = false;
            }
            continue;
        }
        if quote.is_none() && css[i..].starts_with("/*") {
            comment = true;
            continue;
        }
        if matches!(c, '\'' | '"') && (i == 0 || css.as_bytes()[i - 1] != b'\\') {
            if quote == Some(c) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(c);
            }
        }
        if quote.is_some() {
            continue;
        }
        if matches!(c, '{' | '}' | ';') {
            let chunk = &css[start..i];
            if let Some((name, value)) = chunk.split_once(':').filter(|_| depth > 0 && c != '{') {
                let block_start = css[..start].rfind('{').map_or(start, |i| i + 1);
                let block_end = css[start..].find('}').map_or(css.len(), |i| start + i);
                let on_accent = css[block_start..block_end].split(';').any(|decl| {
                    let Some((prop, value)) = decl.split_once(':') else {
                        return false;
                    };
                    prop.trim().starts_with("background")
                        && value.char_indices().any(|(i, _)| {
                            token_at(value, i).is_some_and(|(_, c, _)| {
                                c.alpha > 0.5 && c.chroma() >= 12.0 && Mapping::near(c, m.accent)
                            })
                        })
                });
                out.push_str(name);
                out.push(':');
                out.push_str(&rewrite_value(value, &m, name, on_accent));
            } else if let Some(at) = chunk.find("@define-color") {
                let name_end = chunk[at + 13..].trim_start().find(char::is_whitespace);
                if let Some(n) = name_end {
                    let leading = chunk[at + 13..].len() - chunk[at + 13..].trim_start().len();
                    let end = at + 13 + leading + n;
                    out.push_str(&chunk[..end]);
                    out.push_str(&rewrite_value(&chunk[end..], &m, "", false));
                } else {
                    out.push_str(chunk);
                }
            } else {
                out.push_str(chunk);
            }
            out.push(c);
            if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
            }
            start = i + 1;
        }
    }
    out.push_str(&css[start..]);
    out.push('\n');
    for name in palette.values.keys() {
        if let Some(c) = palette.get(name) {
            let _ = writeln!(out, "@define-color {name} {};", c.css());
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    const BASE: &str = "@define-color theme_bg_color #353535; @define-color theme_fg_color #eeeeec; @define-color theme_text_color white; @define-color theme_base_color #2d2d2d; @define-color theme_selected_bg_color #15539e; @define-color theme_selected_fg_color #ffffff; @define-color borders #1b1b1b; @define-color error_color #cc0000;";
    const NORD: &str = "@define-color theme_bg_color #2E3440; @define-color theme_fg_color #D8DEE9; @define-color theme_text_color @theme_fg_color; @define-color theme_base_color #2E3440; @define-color theme_selected_bg_color #81A1C1; @define-color theme_selected_fg_color rgba(0,0,0,0.87); @define-color borders rgba(216,222,233,0.2); @define-color error_color #BF616A;";
    #[test]
    fn palette_behavior() {
        let p = parse_gtk3_colors(NORD);
        assert_eq!(p.is_dark(), Some(true));
        assert_eq!(
            p.color("theme_text_color"),
            Some("rgb(216, 222, 233)".into())
        );
        assert!(
            parse_gtk3_colors("@define-color a @a;")
                .color("a")
                .is_none()
        );
        assert!(parse_gtk3_colors("button {}").is_empty());
        assert_eq!(Color::parse("#f01"), Color::parse("rgb(255,0,17)"));
        assert!(Color::parse("silver").is_none());
        let out = recolour(
            &format!(
                "{BASE} colorswatch#add-color-button {{ color: #eeeeec; background: #353535; border: 1px solid #1b1b1b; content: \"é #353535\"; }}"
            ),
            &p,
        );
        assert!(out.contains("color: rgb(216, 222, 233)"));
        assert!(out.contains("background: rgb(46, 52, 64)"));
        assert!(out.contains("solid rgba(216, 222, 233, 0.2)"));
        assert!(out.contains("content: \"é #353535\""));
        assert!(out.contains("colorswatch#add-color-button"));
        for (input, want) in [
            ("#929291", "rgb(131, 137, 149)"),
            ("#161616", "rgb(22, 25, 31)"),
            ("#0f3b71", "rgb(92, 115, 138)"),
            ("#660000", "rgb(96, 49, 53)"),
            ("#a0a", "rgb(170, 0, 170)"),
        ] {
            let out = recolour(&format!("{BASE} label {{ color: {input}; }}"), &p);
            assert!(out.contains(want), "{input}: {out}");
        }
        assert_eq!(recolour("button {color: red;}", &p), "button {color: red;}");
        assert_eq!(
            parse_gtk3_colors("@define-color theme_bg_color #f6f5f4;").is_dark(),
            Some(false)
        );
    }
}
