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
            bg_main: Color32::from_rgb(10, 14, 23),
            card_bg: Color32::from_rgb(18, 25, 42),
            card_border: Color32::from_rgb(46, 61, 92),
            text_primary: Color32::from_rgb(255, 255, 255),
            text_secondary: Color32::from_rgb(203, 213, 225),
            accent_blue: Color32::from_rgb(56, 189, 248),
            primary_btn: Color32::from_rgb(37, 99, 235),
            success_green: Color32::from_rgb(34, 197, 94),
            danger_red: Color32::from_rgb(239, 68, 68),
            warn_amber: Color32::from_rgb(245, 158, 11),
            tab_inactive_bg: Color32::from_rgb(24, 33, 54),
            tab_inactive_fg: Color32::from_rgb(226, 232, 240),
            tab_active_bg: Color32::from_rgb(37, 99, 235),
            tab_active_fg: Color32::from_rgb(255, 255, 255),
        }
    }

    pub fn light() -> Self {
        Self {
            is_dark: false,
            bg_main: Color32::from_rgb(243, 246, 250),
            card_bg: Color32::from_rgb(255, 255, 255),
            card_border: Color32::from_rgb(214, 222, 232),
            text_primary: Color32::from_rgb(15, 23, 42),
            text_secondary: Color32::from_rgb(71, 85, 105),
            accent_blue: Color32::from_rgb(2, 132, 199),
            primary_btn: Color32::from_rgb(37, 99, 235),
            success_green: Color32::from_rgb(22, 163, 74),
            danger_red: Color32::from_rgb(220, 38, 38),
            warn_amber: Color32::from_rgb(217, 119, 6),
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

    if let Ok(data) = std::fs::read(r"C:\Windows\Fonts\seguisb.ttf") {
        fonts.font_data.insert(
            "SegoeUI-Semibold".to_owned(),
            Arc::new(egui::FontData::from_owned(data)),
        );
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
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text_primary);

    // Inactive buttons/controls
    visuals.widgets.inactive.bg_fill = palette.tab_inactive_bg;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.card_border);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text_primary);

    // Hovered buttons/controls
    visuals.widgets.hovered.bg_fill = if palette.is_dark {
        Color32::from_rgb(40, 53, 84)
    } else {
        Color32::from_rgb(219, 234, 254)
    };
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent_blue);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text_primary);

    // Active buttons/controls
    visuals.widgets.active.bg_fill = palette.primary_btn;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.primary_btn);
    visuals.widgets.active.corner_radius = CornerRadius::same(8);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    ctx.set_visuals_of(egui_theme, visuals);

    // Spacing & Typography Scale - MEDIUM readable level
    ctx.style_mut_of(egui_theme, |style| {
        style.text_styles = [
            (
                egui::TextStyle::Heading,
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Body,
                egui::FontId::new(14.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Button,
                egui::FontId::new(13.5, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Monospace,
                egui::FontId::new(12.5, egui::FontFamily::Monospace),
            ),
            (
                egui::TextStyle::Small,
                egui::FontId::new(12.0, egui::FontFamily::Proportional),
            ),
        ]
        .into();

        style.spacing.item_spacing = Vec2::new(7.0, 7.0);
        style.spacing.button_padding = Vec2::new(9.0, 5.0);
    });

    palette
}
