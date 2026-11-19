#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config_map;
mod device;
mod protocol;
mod ui;

use config_map::DeviceProfileConfig;
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Vec2};
use std::fs;
use std::sync::Arc;
use ui::ScyroxApp;

const WINDOW_ICON_RGBA: &[u8] = include_bytes!("../assets/icon_64.rgba");

fn load_window_icon() -> Arc<egui::IconData> {
    Arc::new(egui::IconData {
        rgba: WINDOW_ICON_RGBA.to_vec(),
        width: 64,
        height: 64,
    })
}

/// Loads a crisp Windows Japanese system font (Yu Gothic Medium / Regular / Meiryo)
/// so Japanese labels render sharply without bundling large font files.
fn configure_japanese_fonts(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\YuGothM.ttc",
        r"C:\Windows\Fonts\YuGothR.ttc",
        r"C:\Windows\Fonts\meiryo.ttc",
        r"C:\Windows\Fonts\msgothic.ttc",
    ];
    for path in candidates {
        if let Ok(font_bytes) = fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "jp_system".to_owned(),
                egui::FontData::from_owned(font_bytes).into(),
            );
            if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
                family.insert(0, "jp_system".to_owned());
            }
            if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
                family.push("jp_system".to_owned());
            }
            ctx.set_fonts(fonts);
            break;
        }
    }
}

/// Applies a modern dark hardware-utility theme and locks it against Windows Light Mode.
fn apply_modern_theme(ctx: &egui::Context) {
    let text_primary = Color32::from_rgb(240, 246, 252);
    let text_secondary = Color32::from_rgb(205, 214, 224);
    let accent = Color32::from_rgb(56, 189, 248);

    let mut v = egui::Visuals::dark();
    v.dark_mode = true;
    v.panel_fill = Color32::from_rgb(13, 17, 23);
    v.window_fill = Color32::from_rgb(22, 27, 34);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(52, 62, 77));
    v.extreme_bg_color = Color32::from_rgb(9, 12, 17);
    v.faint_bg_color = Color32::from_rgb(20, 25, 33);
    v.override_text_color = Some(text_primary);

    v.selection.bg_fill = Color32::from_rgb(18, 56, 78);
    v.selection.stroke = Stroke::new(1.0, accent);

    v.widgets.noninteractive.bg_fill = Color32::from_rgb(22, 27, 34);
    v.widgets.noninteractive.weak_bg_fill = Color32::from_rgb(22, 27, 34);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(42, 49, 61));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_secondary);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(6);

    v.widgets.inactive.bg_fill = Color32::from_rgb(30, 37, 48);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(30, 37, 48);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(54, 64, 79));
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, text_primary);
    v.widgets.inactive.corner_radius = CornerRadius::same(6);

    v.widgets.hovered.bg_fill = Color32::from_rgb(40, 50, 65);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 50, 65);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    v.widgets.hovered.corner_radius = CornerRadius::same(6);

    v.widgets.active.bg_fill = Color32::from_rgb(20, 62, 86);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(20, 62, 86);
    v.widgets.active.bg_stroke = Stroke::new(1.2, accent);
    v.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    v.widgets.active.corner_radius = CornerRadius::same(6);

    v.widgets.open.bg_fill = Color32::from_rgb(30, 37, 48);
    v.widgets.open.weak_bg_fill = Color32::from_rgb(30, 37, 48);
    v.widgets.open.bg_stroke = Stroke::new(1.0, accent);
    v.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.open.corner_radius = CornerRadius::same(6);

    // Force dark theme even when Windows OS is set to Light Mode.
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v.clone());
    ctx.set_visuals_of(egui::Theme::Light, v.clone());

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.spacing.interact_size = Vec2::new(36.0, 26.0);
    style.spacing.window_margin = egui::Margin::same(10);
    style.text_styles = [
        (TextStyle::Heading, FontId::new(17.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(12.5, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
    ]
    .into();
    style.visuals = v;
    ctx.set_style(style);
}

fn main() -> eframe::Result<()> {
    let config = DeviceProfileConfig::load_or_default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([940.0, 640.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("Scyrox V6 Control")
            .with_icon(load_window_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Scyrox V6 Control",
        options,
        Box::new(move |cc| {
            apply_modern_theme(&cc.egui_ctx);
            configure_japanese_fonts(&cc.egui_ctx);
            Ok(Box::new(ScyroxApp::new(config)))
        }),
    )
}
