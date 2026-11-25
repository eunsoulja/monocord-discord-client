//! Color, spacing, and type tokens mapped onto egui's `Style`/`Visuals`.
//!
//! Colors are resolved through a runtime [`Palette`] so themes can be switched
//! without recompiling (Legcord-style theme presets + accent override).

use std::sync::{Arc, RwLock};

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, Stroke, Style, Visuals, WidgetText};

/// Runtime color palette. Every `color::*()` accessor reads the active one.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Main background.
    pub bg: Color32,
    /// Server rail / far-left strip (darker than `bg`).
    pub rail: Color32,
    /// Secondary background.
    pub bg_secondary: Color32,
    /// Panel background.
    pub panel: Color32,
    /// Elevated background.
    pub elevated: Color32,
    /// Border.
    pub border: Color32,
    /// Subtle border.
    pub border_subtle: Color32,
    /// Primary text.
    pub text: Color32,
    /// Secondary text.
    pub text_secondary: Color32,
    /// Muted text.
    pub text_muted: Color32,
    /// Accent color.
    pub accent: Color32,
    /// Hover wash.
    pub hover: Color32,
    /// Active/selected wash.
    pub active: Color32,
    /// Error indicator.
    pub error: Color32,
    /// Warning indicator.
    pub warning: Color32,
    /// Success indicator.
    pub success: Color32,
}

impl Palette {
    /// The default monochrome palette (terminal identity).
    pub const MONO: Self = Self {
        bg: Color32::from_rgb(0x09, 0x09, 0x09),
        rail: Color32::from_rgb(0x06, 0x06, 0x06),
        bg_secondary: Color32::from_rgb(0x0D, 0x0D, 0x0D),
        panel: Color32::from_rgb(0x10, 0x10, 0x10),
        elevated: Color32::from_rgb(0x14, 0x14, 0x14),
        border: Color32::from_rgb(0x25, 0x25, 0x25),
        border_subtle: Color32::from_rgb(0x1B, 0x1B, 0x1B),
        text: Color32::from_rgb(0xF2, 0xF2, 0xF2),
        text_secondary: Color32::from_rgb(0x88, 0x88, 0x88),
        text_muted: Color32::from_rgb(0x55, 0x55, 0x55),
        accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        hover: Color32::from_rgba_premultiplied(255, 255, 255, 10),
        active: Color32::from_rgba_premultiplied(255, 255, 255, 18),
        error: Color32::from_rgb(0xE5, 0x4B, 0x4B),
        warning: Color32::from_rgb(0xC9, 0xA2, 0x27),
        success: Color32::from_rgb(0x7F, 0xB0, 0x69),
    };
}

/// Built-in theme presets (Legcord-style theme picker).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePreset {
    /// Monochrome terminal theme (default).
    Mono,
    /// Discord-inspired dark theme (blurple accent).
    DiscordDark,
    /// Discord-inspired light theme.
    Light,
    /// Pure-black OLED theme.
    Amoled,
}

impl ThemePreset {
    /// All presets in display order.
    pub const ALL: [ThemePreset; 4] = [
        Self::Mono,
        Self::DiscordDark,
        Self::Light,
        Self::Amoled,
    ];

    /// Display label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Mono => "MONO",
            Self::DiscordDark => "DISCORD DARK",
            Self::Light => "LIGHT",
            Self::Amoled => "AMOLED",
        }
    }
}

fn hover_active(accent: Color32) -> (Color32, Color32) {
    let [r, g, b, _] = accent.to_array();
    (
        Color32::from_rgba_premultiplied(r, g, b, 24),
        Color32::from_rgba_premultiplied(r, g, b, 48),
    )
}

/// Build the palette for a preset, optionally overriding the accent color.
#[must_use]
pub fn palette_for(preset: ThemePreset, accent: Option<Color32>) -> Palette {
    let mut p = match preset {
        ThemePreset::Mono => Palette::MONO,
        ThemePreset::DiscordDark => Palette {
            bg: Color32::from_rgb(0x1E, 0x1F, 0x22),
            rail: Color32::from_rgb(0x19, 0x1A, 0x1D),
            bg_secondary: Color32::from_rgb(0x2B, 0x2D, 0x31),
            panel: Color32::from_rgb(0x2B, 0x2D, 0x31),
            elevated: Color32::from_rgb(0x38, 0x3A, 0x40),
            border: Color32::from_rgb(0x3F, 0x41, 0x47),
            border_subtle: Color32::from_rgb(0x34, 0x36, 0x3B),
            text: Color32::from_rgb(0xDB, 0xDE, 0xE1),
            text_secondary: Color32::from_rgb(0xB5, 0xB9, 0xC1),
            text_muted: Color32::from_rgb(0x94, 0x9B, 0xA4),
            accent: Color32::from_rgb(0x58, 0x65, 0xF2),
            hover: Color32::TRANSPARENT,
            active: Color32::TRANSPARENT,
            error: Color32::from_rgb(0xED, 0x42, 0x45),
            warning: Color32::from_rgb(0xF0, 0xB2, 0x32),
            success: Color32::from_rgb(0x23, 0xA5, 0x5A),
        },
        ThemePreset::Light => Palette {
            bg: Color32::from_rgb(0xF2, 0xF3, 0xF5),
            rail: Color32::from_rgb(0xE6, 0xE7, 0xEA),
            bg_secondary: Color32::from_rgb(0xE3, 0xE5, 0xE8),
            panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            elevated: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            border: Color32::from_rgb(0xD8, 0xDB, 0xDF),
            border_subtle: Color32::from_rgb(0xE3, 0xE5, 0xE8),
            text: Color32::from_rgb(0x31, 0x33, 0x38),
            text_secondary: Color32::from_rgb(0x4E, 0x50, 0x55),
            text_muted: Color32::from_rgb(0x80, 0x84, 0x8A),
            accent: Color32::from_rgb(0x58, 0x65, 0xF2),
            hover: Color32::TRANSPARENT,
            active: Color32::TRANSPARENT,
            error: Color32::from_rgb(0xDA, 0x37, 0x3B),
            warning: Color32::from_rgb(0xB8, 0x86, 0x0B),
            success: Color32::from_rgb(0x1E, 0x8A, 0x46),
        },
        ThemePreset::Amoled => Palette {
            bg: Color32::BLACK,
            rail: Color32::BLACK,
            bg_secondary: Color32::from_rgb(0x05, 0x05, 0x05),
            panel: Color32::from_rgb(0x0A, 0x0A, 0x0A),
            elevated: Color32::from_rgb(0x14, 0x14, 0x14),
            border: Color32::from_rgb(0x22, 0x22, 0x22),
            border_subtle: Color32::from_rgb(0x16, 0x16, 0x16),
            text: Color32::from_rgb(0xF2, 0xF2, 0xF2),
            text_secondary: Color32::from_rgb(0x88, 0x88, 0x88),
            text_muted: Color32::from_rgb(0x55, 0x55, 0x55),
            accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            hover: Color32::TRANSPARENT,
            active: Color32::TRANSPARENT,
            error: Color32::from_rgb(0xE5, 0x4B, 0x4B),
            warning: Color32::from_rgb(0xC9, 0xA2, 0x27),
            success: Color32::from_rgb(0x7F, 0xB0, 0x69),
        },
    };
    if let Some(a) = accent {
        p.accent = a;
    }
    // Hover/active washes derive from the accent so recoloring propagates.
    let (hover, active) = hover_active(p.accent);
    // Mono/AMOLED keep their near-white washes only when accent is white;
    // otherwise accent-tinted washes read better.
    if p.accent == Color32::WHITE || p.accent == Palette::MONO.accent {
        p.hover = Palette::MONO.hover;
        p.active = Palette::MONO.active;
    } else {
        p.hover = hover;
        p.active = active;
    }
    p
}

static PALETTE: RwLock<Palette> = RwLock::new(Palette::MONO);

/// The currently active palette.
#[must_use]
pub fn palette() -> Palette {
    PALETTE.read().map_or(Palette::MONO, |p| *p)
}

/// Replace the active palette. Called by `apply_theme` each frame.
pub fn set_palette(p: Palette) {
    if let Ok(mut g) = PALETTE.write() {
        *g = p;
    }
}

/// Runtime color accessors (drop-in replacement for the old consts).
pub mod color {
    use super::palette;
    use egui::Color32;

    /// Main background.
    #[must_use]
    pub fn bg() -> Color32 {
        palette().bg
    }
    /// Server rail background (darkest surface).
    #[must_use]
    pub fn rail() -> Color32 {
        palette().rail
    }
    /// Secondary background.
    #[must_use]
    pub fn bg_secondary() -> Color32 {
        palette().bg_secondary
    }
    /// Panel background.
    #[must_use]
    pub fn panel() -> Color32 {
        palette().panel
    }
    /// Elevated background.
    #[must_use]
    pub fn elevated() -> Color32 {
        palette().elevated
    }
    /// Border.
    #[must_use]
    pub fn border() -> Color32 {
        palette().border
    }
    /// Subtle border.
    #[must_use]
    pub fn border_subtle() -> Color32 {
        palette().border_subtle
    }
    /// Primary text.
    #[must_use]
    pub fn text() -> Color32 {
        palette().text
    }
    /// Secondary text.
    #[must_use]
    pub fn text_secondary() -> Color32 {
        palette().text_secondary
    }
    /// Muted text.
    #[must_use]
    pub fn text_muted() -> Color32 {
        palette().text_muted
    }
    /// Accent.
    #[must_use]
    pub fn accent() -> Color32 {
        palette().accent
    }
    /// Hover wash.
    #[must_use]
    pub fn hover() -> Color32 {
        palette().hover
    }
    /// Active/selected wash.
    #[must_use]
    pub fn active() -> Color32 {
        palette().active
    }
    /// Error indicator.
    #[must_use]
    pub fn error() -> Color32 {
        palette().error
    }
    /// Warning indicator.
    #[must_use]
    pub fn warning() -> Color32 {
        palette().warning
    }
    /// Success indicator.
    #[must_use]
    pub fn success() -> Color32 {
        palette().success
    }
}

/// Spacing scale in points.
pub mod space {
    /// 2px.
    pub const XS: f32 = 2.0;
    /// 4px.
    pub const SM: f32 = 4.0;
    /// 8px.
    pub const MD: f32 = 8.0;
    /// 12px.
    pub const LG: f32 = 12.0;
    /// 16px.
    pub const XL: f32 = 16.0;
    /// 24px.
    pub const XXL: f32 = 24.0;
}

/// Type scale helpers (sizes are multiplied by the configured font size).
#[derive(Debug, Clone, Copy)]
pub struct TypeScale {
    /// Base font size in points.
    pub base: f32,
}

impl TypeScale {
    /// Caption / metadata.
    #[must_use]
    pub fn caption(self) -> f32 {
        self.base * 0.85
    }
    /// Body.
    #[must_use]
    pub fn body(self) -> f32 {
        self.base
    }
    /// Emphasized body.
    #[must_use]
    pub fn strong(self) -> f32 {
        self.base * 1.1
    }
    /// Section label (uppercase handled by caller).
    #[must_use]
    pub fn section(self) -> f32 {
        self.base * 0.75
    }
    /// Title.
    #[must_use]
    pub fn title(self) -> f32 {
        self.base * 1.6
    }
    /// Heading.
    #[must_use]
    pub fn heading(self) -> f32 {
        self.base * 2.2
    }
}

/// Runtime theme configuration supplied by the app.
#[derive(Debug, Clone, Copy)]
pub struct ThemeConfig {
    /// Base font size in points.
    pub font_size: f32,
    /// Compact density.
    pub compact: bool,
    /// UI scale percent (100 = 1.0).
    pub ui_scale: f32,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            font_size: 13.0,
            compact: false,
            ui_scale: 100.0,
        }
    }
}

/// Snap a rectangle edge to the physical pixel grid for crisp 1px lines.
#[must_use]
pub fn snap_rect(rect: Rect, pixels_per_point: f32) -> Rect {
    let snap = |v: f32| (v * pixels_per_point).round() / pixels_per_point;
    Rect::from_min_max(
        Pos2::new(snap(rect.min.x), snap(rect.min.y)),
        Pos2::new(snap(rect.max.x), snap(rect.max.y)),
    )
}

/// Apply the active palette + metrics to egui's `Style` and install it.
pub fn apply_theme(ctx: &egui::Context, cfg: ThemeConfig) {
    let p = palette();
    let scale = cfg.ui_scale / 100.0;
    let mut style = Style {
        visuals: visuals(&p),
        ..Style::default()
    };
    style.spacing.item_spacing = egui::vec2(space::SM, if cfg.compact { 2.0 } else { space::XS });
    style.spacing.button_padding = egui::vec2(space::MD, if cfg.compact { 2.0 } else { space::XS });
    style.spacing.interact_size = egui::vec2(28.0, if cfg.compact { 20.0 } else { 24.0 });
    style.spacing.scroll = egui::style::ScrollStyle::thin();
    style.spacing.window_margin = egui::Margin::same(12);
    style.spacing.menu_margin = egui::Margin::same(4);

    let base = cfg.font_size * scale;
    let mono = egui::FontFamily::Monospace;
    style.text_styles = [
        (
            egui::TextStyle::Small,
            FontId::new(base * 0.85, mono.clone()),
        ),
        (egui::TextStyle::Body, FontId::new(base, mono.clone())),
        (egui::TextStyle::Button, FontId::new(base, mono.clone())),
        (
            egui::TextStyle::Heading,
            FontId::new(base * 1.4, mono.clone()),
        ),
        (egui::TextStyle::Monospace, FontId::new(base, mono.clone())),
    ]
    .into_iter()
    .collect();

    ctx.options_mut(|o| {
        o.zoom_factor = scale;
        o.tessellation_options.feathering = false;
    });
    let luminance = u32::from(p.bg.r()) + u32::from(p.bg.g()) + u32::from(p.bg.b());
    let dark = luminance < 384;
    ctx.set_theme(if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    let style = Arc::new(style);
    ctx.all_styles_mut(|s| *s = (*style).clone());
}

/// Crisp visuals derived from the active palette.
#[must_use]
pub fn visuals(p: &Palette) -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.bg;
    v.faint_bg_color = p.bg_secondary;
    v.code_bg_color = p.bg_secondary;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.selection.bg_fill = p.active;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.text;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.error;

    v.widgets.noninteractive.bg_fill = p.bg_secondary;
    v.widgets.noninteractive.bg_stroke = Stroke::NONE;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text_secondary);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(2);

    v.widgets.inactive.bg_fill = p.bg_secondary;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.corner_radius = CornerRadius::same(2);
    v.widgets.inactive.expansion = 0.0;

    v.widgets.hovered.bg_fill = p.elevated;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.hovered.corner_radius = CornerRadius::same(2);

    v.widgets.active.bg_fill = p.elevated;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.corner_radius = CornerRadius::same(2);

    v.widgets.open.bg_fill = p.elevated;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.open.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.open.corner_radius = CornerRadius::same(2);

    v
}

/// Small uppercase section label, e.g. `SYSTEM STATUS`.
#[must_use]
pub fn section_label(text: &str) -> WidgetText {
    WidgetText::from(format!(" {} ", text.to_uppercase()))
        .small()
        .color(color::text_muted())
}

/// Thin horizontal separator snapped to the pixel grid.
pub fn separator(ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    let y = snap_rect(rect, ui.ctx().pixels_per_point()).center().y;
    let painter = ui.painter();
    let x0 = rect.left();
    let x1 = rect.right();
    painter.hline(x0..=x1, y, Stroke::new(1.0, color::border_subtle()));
    ui.allocate_space(egui::vec2(0.0, 1.0));
}
