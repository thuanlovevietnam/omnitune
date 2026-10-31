use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Theme, Vec2};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ThemePalette {
    pub is_dark: bool,
    pub bg_main: Color32,
    pub card_bg: Color32,
    pub card_border: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub accent_blue: Color32,
    pub primary_btn: Color32,
    pub success_green: Color32,
    pub danger_red: Color32,
    pub warn_amber: Color32,
    pub tab_inactive_bg: Color32,
    pub tab_inactive_fg: Color32,
    pub tab_active_bg: Color32,
    pub tab_active_fg: Color32,
}

impl ThemePalette {
    pub fn dark() -> Self {
        Self {
            is_dark: true,
            bg_main: Color32::from_rgb(11, 15, 25),          // Deep dark navy
            card_bg: Color32::from_rgb(20, 28, 45),          // Slate dark card
            card_border: Color32::from_rgb(50, 68, 100),     // Crisp border
            text_primary: Color32::from_rgb(255, 255, 255),  // Pure white (Max contrast)
            text_secondary: Color32::from_rgb(203, 213, 225),// Bright silver
            accent_blue: Color32::from_rgb(56, 189, 248),    // Neon sky blue
            primary_btn: Color32::from_rgb(37, 99, 235),    // Vibrant blue
            success_green: Color32::from_rgb(34, 197, 94),   // Green
            danger_red: Color32::from_rgb(239, 68, 68),      // Red
            warn_amber: Color32::from_rgb(245, 158, 11),     // Amber
            tab_inactive_bg: Color32::from_rgb(28, 38, 62),
            tab_inactive_fg: Color32::from_rgb(226, 232, 240),
            tab_active_bg: Color32::from_rgb(37, 99, 235),
            tab_active_fg: Color32::from_rgb(255, 255, 255),
        }
    }

    pub fn light() -> Self {
        Self {
            is_dark: false,
            bg_main: Color32::from_rgb(241, 245, 249),       // Slate-100 clean background
            card_bg: Color32::from_rgb(255, 255, 255),       // Pure white card
            card_border: Color32::from_rgb(203, 213, 225),   // Slate-300 crisp border
            text_primary: Color32::from_rgb(15, 23, 42),     // Slate-900 jet black (AAA contrast)
            text_secondary: Color32::from_rgb(51, 65, 85),   // Slate-700 dark gray (Clear & readable)
            accent_blue: Color32::from_rgb(29, 78, 216),     // Deep royal blue (High contrast on white)
            primary_btn: Color32::from_rgb(37, 99, 235),     // Blue button
            success_green: Color32::from_rgb(22, 163, 74),   // Deep green
            danger_red: Color32::from_rgb(220, 38, 38),      // Deep red
            warn_amber: Color32::from_rgb(217, 119, 6),      // Deep amber
            tab_inactive_bg: Color32::from_rgb(226, 232, 240),
            tab_inactive_fg: Color32::from_rgb(30, 41, 59),
            tab_active_bg: Color32::from_rgb(37, 99, 235),
            tab_active_fg: Color32::from_rgb(255, 255, 255),
        }
    }

    pub fn modern_card<R>(
        &self,
        ui: &mut egui::Ui,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        egui::Frame::new()
            .inner_margin(Margin::same(10))
            .corner_radius(CornerRadius::same(8))
            .fill(self.card_bg)
            .stroke(Stroke::new(1.0, self.card_border))
            .show(ui, add_contents)
    }
}

pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 1. Primary UI Font: Windows Segoe UI
    if let Ok(data) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert(
            "SegoeUI".to_owned(),
            Arc::new(egui::FontData::from_owned(data)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "SegoeUI".to_owned());
    }

    // 2. Semibold UI Font
    if let Ok(data) = std::fs::read(r"C:\Windows\Fonts\seguisb.ttf") {
        fonts.font_data.insert(
            "SegoeUI-Semibold".to_owned(),
            Arc::new(egui::FontData::from_owned(data)),
        );
    }

    // 3. Fallback Symbol Font for Windows UI icons (prevents missing glyph squares)
    if let Ok(data) = std::fs::read(r"C:\Windows\Fonts\seguisym.ttf") {
        fonts.font_data.insert(
            "SegoeSymbol".to_owned(),
            Arc::new(egui::FontData::from_owned(data)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("SegoeSymbol".to_owned());
    }

    ctx.set_fonts(fonts);
}

pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode) -> ThemePalette {
    let palette = match mode {
        ThemeMode::Dark => ThemePalette::dark(),
        ThemeMode::Light => ThemePalette::light(),
    };

    let egui_theme = if palette.is_dark {
        Theme::Dark
    } else {
        Theme::Light
    };

    let mut visuals = if palette.is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    visuals.override_text_color = Some(palette.text_primary);
    visuals.panel_fill = palette.bg_main;
    visuals.window_fill = palette.bg_main;

    // Non-interactive widgets
    visuals.widgets.noninteractive.bg_fill = palette.card_bg;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.card_border);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text_primary);

    // Inactive controls
    visuals.widgets.inactive.bg_fill = palette.tab_inactive_bg;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.card_border);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text_primary);

    // Hovered controls
    visuals.widgets.hovered.bg_fill = if palette.is_dark {
        Color32::from_rgb(38, 52, 82)
    } else {
        Color32::from_rgb(219, 234, 254)
    };
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent_blue);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text_primary);

    // Active controls
    visuals.widgets.active.bg_fill = palette.primary_btn;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.primary_btn);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    ctx.set_visuals_of(egui_theme, visuals);

    // Spacing & Typography Scale
    ctx.style_mut_of(egui_theme, |style| {
        style.text_styles = [
            (
                egui::TextStyle::Heading,
                egui::FontId::new(17.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Body,
                egui::FontId::new(13.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Button,
                egui::FontId::new(12.5, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Monospace,
                egui::FontId::new(12.0, egui::FontFamily::Monospace),
            ),
            (
                egui::TextStyle::Small,
                egui::FontId::new(11.0, egui::FontFamily::Proportional),
            ),
        ]
        .into();

        style.spacing.item_spacing = Vec2::new(6.0, 6.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
    });

    palette
}
