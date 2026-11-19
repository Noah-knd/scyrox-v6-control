//! Modern, responsive, high-contrast native Windows GUI for Scyrox V6 Control.

use crate::config_map::{
    DeviceProfileConfig, LiftOffDistance, MouseSettings, PollingRate, SensorRunMode,
};
use crate::device::ScyroxDeviceManager;
use crate::protocol::{build_packet, format_hex, parse_hex};
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Sense, Stroke, Vec2};
use std::fs;
use std::time::{Duration, Instant};

const BG_SIDEBAR: Color32 = Color32::from_rgb(16, 20, 27);
const BG_CARD: Color32 = Color32::from_rgb(22, 27, 34);
const BG_CARD_ACTIVE: Color32 = Color32::from_rgb(17, 34, 48);
const BG_BTN: Color32 = Color32::from_rgb(32, 39, 51);
const BORDER_CARD: Color32 = Color32::from_rgb(44, 52, 65);
const ACCENT: Color32 = Color32::from_rgb(56, 189, 248);
const ACCENT_DARK: Color32 = Color32::from_rgb(14, 116, 144);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 246, 252);
const TEXT_MUTED: Color32 = Color32::from_rgb(154, 165, 177);
const GREEN: Color32 = Color32::from_rgb(34, 197, 94);
const AMBER: Color32 = Color32::from_rgb(245, 158, 11);
const RED: Color32 = Color32::from_rgb(239, 68, 68);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveTab {
    Dpi,
    Performance,
    Profiles,
    Diagnostics,
}

pub struct ScyroxApp {
    manager: ScyroxDeviceManager,
    settings: MouseSettings,
    active_tab: ActiveTab,
    status_banner: String,
    status_is_error: bool,
    last_battery_poll: Instant,

    // Diagnostics tab state
    dump_start_hex: String,
    dump_length: u16,
    dump_output: String,
    raw_cmd_hex: String,
    raw_payload_hex: String,

    // Profile JSON export/import path
    profile_path: String,
}

impl ScyroxApp {
    pub fn new(config: DeviceProfileConfig) -> Self {
        let mut manager = ScyroxDeviceManager::new(config);
        let mut settings = MouseSettings::default();

        let (status_banner, status_is_error) = match manager.connect() {
            Ok(()) => {
                if let Ok(read_cfg) = manager.read_settings() {
                    settings = read_cfg;
                }
                (
                    "Scyrox V6に接続しました（初期状態は読み取り専用です）".to_string(),
                    false,
                )
            }
            Err(err) => (format!("未接続: {err}"), false),
        };

        Self {
            manager,
            settings,
            active_tab: ActiveTab::Dpi,
            status_banner,
            status_is_error,
            last_battery_poll: Instant::now(),
            dump_start_hex: "0000".to_string(),
            dump_length: 192,
            dump_output: String::new(),
            raw_cmd_hex: "04".to_string(),
            raw_payload_hex: String::new(),
            profile_path: "scyrox_v6_profile.json".to_string(),
        }
    }

    fn set_status(&mut self, msg: impl Into<String>, is_error: bool) {
        self.status_banner = msg.into();
        self.status_is_error = is_error;
    }

    /// Renders a clean dark rounded card container.
    fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::new()
            .fill(BG_CARD)
            .stroke(Stroke::new(1.0, BORDER_CARD))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add_contents(ui);
            });
    }

    /// Renders a highlightable card (used for active DPI stage / sensor mode / profile).
    fn selectable_card(
        ui: &mut egui::Ui,
        active: bool,
        accent_color: Color32,
        add_contents: impl FnOnce(&mut egui::Ui),
    ) {
        let fill = if active { BG_CARD_ACTIVE } else { BG_CARD };
        let stroke = if active {
            Stroke::new(1.5, accent_color)
        } else {
            Stroke::new(1.0, BORDER_CARD)
        };
        egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add_contents(ui);
            });
    }

    /// Renders a crisp circular `?` badge that shows a helpful explanation tooltip on hover.
    fn help_badge(ui: &mut egui::Ui, tooltip: &str) {
        let size = Vec2::splat(16.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        if ui.is_rect_visible(rect) {
            let hovered = response.hovered();
            let fill = if hovered {
                ACCENT_DARK
            } else {
                Color32::from_rgb(38, 46, 58)
            };
            let stroke_color = if hovered { ACCENT } else { Color32::from_rgb(80, 92, 110) };
            let text_color = if hovered { Color32::WHITE } else { TEXT_MUTED };
            let center = rect.center();
            ui.painter().circle(center, 7.5, fill, Stroke::new(1.0, stroke_color));
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(10.5),
                text_color,
            );
        }
        response.on_hover_ui(|ui| {
            ui.set_max_width(280.0);
            ui.label(RichText::new(tooltip).size(12.0).color(TEXT_PRIMARY));
        });
    }

    /// Styled dark secondary button with guaranteed high-contrast text.
    fn dark_button(label: &str, min_size: Vec2) -> egui::Button<'_> {
        egui::Button::new(RichText::new(label).color(TEXT_PRIMARY))
            .fill(BG_BTN)
            .stroke(Stroke::new(1.0, BORDER_CARD))
            .min_size(min_size)
    }

    /// Draws a modern pill-style toggle switch.
    fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
        let desired_size = Vec2::new(38.0, 20.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
            let radius = rect.height() * 0.5;
            let bg_color = if *on {
                ACCENT
            } else {
                Color32::from_rgb(52, 61, 75)
            };
            ui.painter()
                .rect_filled(rect, CornerRadius::same(radius as u8), bg_color);
            let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
            let center = egui::pos2(circle_x, rect.center().y);
            ui.painter()
                .circle_filled(center, radius - 3.0, Color32::WHITE);
        }
        response
    }

    /// Draws a full-width sidebar navigation button with a left accent indicator.
    fn nav_button(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
        let desired = Vec2::new(ui.available_width(), 34.0);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
        if ui.is_rect_visible(rect) {
            let bg = if selected {
                Color32::from_rgb(20, 42, 58)
            } else if response.hovered() {
                Color32::from_rgb(26, 33, 44)
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(rect, CornerRadius::same(7), bg);
            if selected {
                let bar = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 2.0, rect.top() + 7.0),
                    Vec2::new(3.0, rect.height() - 14.0),
                );
                ui.painter()
                    .rect_filled(bar, CornerRadius::same(2), ACCENT);
            }
            let text_color = if selected { TEXT_PRIMARY } else { TEXT_MUTED };
            ui.painter().text(
                egui::pos2(rect.left() + 14.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(13.0),
                text_color,
            );
        }
        response.clicked()
    }

    /// Draws a compact battery bar inside the sidebar device card.
    fn render_battery_bar(ui: &mut egui::Ui, percent: u8, charging: bool, voltage_mv: u16) {
        let color = if percent <= 15 {
            RED
        } else if percent <= 35 {
            AMBER
        } else {
            GREEN
        };

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{percent}%"))
                    .size(15.0)
                    .strong()
                    .color(color),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tag = if charging { "充電/有線" } else { "無線" };
                ui.label(
                    RichText::new(format!("{tag} · {voltage_mv}mV"))
                        .size(10.5)
                        .color(TEXT_MUTED),
                );
            });
        });

        let bar_h = 6.0;
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), bar_h), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(38, 45, 56));
        let fill_w = (rect.width() * (percent.min(100) as f32 / 100.0)).max(2.0);
        let fill_rect = egui::Rect::from_min_size(rect.min, Vec2::new(fill_w, bar_h));
        ui.painter()
            .rect_filled(fill_rect, CornerRadius::same(3), color);
    }

    fn render_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav_sidebar")
            .resizable(false)
            .exact_width(198.0)
            .frame(
                egui::Frame::new()
                    .fill(BG_SIDEBAR)
                    .stroke(Stroke::new(1.0, BORDER_CARD))
                    .inner_margin(Margin::same(12)),
            )
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("SCYROX V6")
                        .size(18.0)
                        .strong()
                        .color(TEXT_PRIMARY),
                );
                ui.label(
                    RichText::new("RUST CONTROL UTILITY")
                        .size(10.0)
                        .color(ACCENT),
                );
                ui.add_space(10.0);

                if Self::nav_button(ui, "DPI ステージ", self.active_tab == ActiveTab::Dpi) {
                    self.active_tab = ActiveTab::Dpi;
                }
                if Self::nav_button(
                    ui,
                    "パフォーマンス",
                    self.active_tab == ActiveTab::Performance,
                ) {
                    self.active_tab = ActiveTab::Performance;
                }
                if Self::nav_button(ui, "プロファイル", self.active_tab == ActiveTab::Profiles) {
                    self.active_tab = ActiveTab::Profiles;
                }
                if Self::nav_button(
                    ui,
                    "HID 診断・ダンプ",
                    self.active_tab == ActiveTab::Diagnostics,
                ) {
                    self.active_tab = ActiveTab::Diagnostics;
                }

                ui.add_space(10.0);

                // Device status card
                egui::Frame::new()
                    .fill(BG_CARD)
                    .stroke(Stroke::new(1.0, BORDER_CARD))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if let Some(info) = &self.manager.info {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("●").color(GREEN).size(11.0));
                                ui.label(
                                    RichText::new(if self.manager.demo_mode {
                                        "デモ接続"
                                    } else {
                                        "接続中"
                                    })
                                    .strong()
                                    .color(TEXT_PRIMARY),
                                );
                            });
                            ui.label(
                                RichText::new(&info.connection_label)
                                    .size(11.0)
                                    .color(TEXT_MUTED),
                            );
                            if let Some(hs) = &info.handshake {
                                ui.label(
                                    RichText::new(format!(
                                        "MID {} · FW v{}.{}",
                                        hs.model_id, hs.firmware_major, hs.firmware_minor
                                    ))
                                    .size(10.5)
                                    .color(TEXT_MUTED),
                                );
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("○").color(AMBER).size(11.0));
                                ui.label(RichText::new("未接続").strong().color(AMBER));
                            });
                            ui.label(
                                RichText::new("VID 0x3554 を待機中")
                                    .size(10.5)
                                    .color(TEXT_MUTED),
                            );
                        }

                        if let Some(bat) = self.manager.battery {
                            ui.add_space(6.0);
                            Self::render_battery_bar(
                                ui,
                                bat.percent,
                                bat.wired_or_charging,
                                bat.voltage_mv,
                            );
                        }

                        ui.add_space(8.0);
                        ui.columns(2, |cols| {
                            if cols[0]
                                .add(Self::dark_button(
                                    "再接続",
                                    Vec2::new(cols[0].available_width(), 26.0),
                                ))
                                .clicked()
                            {
                                self.manager.demo_mode = false;
                                match self.manager.connect() {
                                    Ok(()) => {
                                        if let Ok(cfg) = self.manager.read_settings() {
                                            self.settings = cfg;
                                        }
                                        self.set_status("デバイスへ接続しました", false);
                                    }
                                    Err(e) => self.set_status(format!("接続失敗: {e}"), true),
                                }
                            }
                            if cols[1]
                                .add_enabled(
                                    self.manager.is_connected(),
                                    Self::dark_button(
                                        "残量更新",
                                        Vec2::new(cols[1].available_width(), 26.0),
                                    ),
                                )
                                .clicked()
                            {
                                match self.manager.refresh_battery() {
                                    Ok(b) => self.set_status(
                                        format!("バッテリー: {}% ({} mV)", b.percent, b.voltage_mv),
                                        false,
                                    ),
                                    Err(e) => self.set_status(format!("取得失敗: {e}"), true),
                                }
                            }
                        });

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("デモモード").size(11.5).color(TEXT_MUTED));
                            Self::help_badge(
                                ui,
                                "実機を接続せずに仮想マウスでUIの動作を試せるモードです。オンの間は実機へ一切HID通信を行いません。",
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let mut demo = self.manager.demo_mode;
                                    if Self::toggle_switch(ui, &mut demo).changed() {
                                        self.manager.enable_demo_mode(demo);
                                        if demo {
                                            self.set_status("デモモード有効（仮想Scyrox V6）", false);
                                        } else {
                                            self.set_status("デモモードを終了しました", false);
                                        }
                                    }
                                },
                            );
                        });
                    });

                // Bottom-anchored write controls
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    let apply_enabled = self.manager.is_connected()
                        && (self.manager.write_unlocked || self.manager.demo_mode);
                    let write_btn = egui::Button::new(
                        RichText::new("デバイスへ書き込む")
                            .strong()
                            .color(if apply_enabled {
                                Color32::WHITE
                            } else {
                                TEXT_MUTED
                            }),
                    )
                    .fill(if apply_enabled {
                        ACCENT_DARK
                    } else {
                        Color32::from_rgb(28, 34, 44)
                    })
                    .stroke(Stroke::new(
                        1.0,
                        if apply_enabled { ACCENT } else { BORDER_CARD },
                    ))
                    .min_size(Vec2::new(ui.available_width(), 32.0));

                    if ui.add_enabled(apply_enabled, write_btn).clicked() {
                        match self.manager.apply_settings(&self.settings) {
                            Ok(()) => self.set_status("設定をデバイスへ書き込みました", false),
                            Err(e) => self.set_status(format!("書き込み失敗: {e}"), true),
                        }
                    }

                    let read_btn = Self::dark_button(
                        "本体から設定読出",
                        Vec2::new(ui.available_width(), 28.0),
                    );
                    if ui.add_enabled(self.manager.is_connected(), read_btn).clicked() {
                        match self.manager.read_settings() {
                            Ok(cfg) => {
                                self.settings = cfg;
                                self.set_status("本体フラッシュから設定を読み出しました", false);
                            }
                            Err(e) => self.set_status(format!("読出失敗: {e}"), true),
                        }
                    }

                    ui.add_space(4.0);
                    egui::Frame::new()
                        .fill(if self.manager.write_unlocked {
                            Color32::from_rgb(56, 26, 26)
                        } else {
                            BG_CARD
                        })
                        .stroke(Stroke::new(
                            1.0,
                            if self.manager.write_unlocked {
                                RED
                            } else {
                                BORDER_CARD
                            },
                        ))
                        .corner_radius(CornerRadius::same(7))
                        .inner_margin(Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let label = if self.manager.write_unlocked {
                                    RichText::new("書込許可").size(11.5).strong().color(RED)
                                } else {
                                    RichText::new("書込保護中")
                                        .size(11.5)
                                        .color(TEXT_MUTED)
                                };
                                ui.label(label);
                                Self::help_badge(
                                    ui,
                                    "誤書き込み防止ロックです。初回は「HID診断」タブで応答やダンプを確認し、公式S-Centerを閉じた状態でオンにしてください。",
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        Self::toggle_switch(ui, &mut self.manager.write_unlocked);
                                    },
                                );
                            });
                        });
                });
            });
    }

    fn render_dpi_tab(&mut self, ui: &mut egui::Ui) {
        let active_idx = self
            .settings
            .active_dpi_stage
            .min(self.settings.dpi_stage_count.saturating_sub(1));
        self.settings.active_dpi_stage = active_idx;
        let current_dpi = self.settings.dpi_stages[active_idx].dpi;
        let current_rgb = self.settings.dpi_stages[active_idx].color;
        let current_color = Color32::from_rgb(current_rgb[0], current_rgb[1], current_rgb[2]);

        // Compact hero summary strip
        Self::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("アクティブ DPI (Stage {})", active_idx + 1))
                                .size(11.5)
                                .color(TEXT_MUTED),
                        );
                        Self::help_badge(
                            ui,
                            "DPIは1インチ動かしたときのカーソル移動量です。値が大きいほど少ない手の動きで速く動きます（PAW3950は50〜30,000 DPI・50刻み）。",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{current_dpi}"))
                                .size(28.0)
                                .strong()
                                .color(ACCENT),
                        );
                        ui.label(RichText::new("DPI").size(13.0).color(TEXT_MUTED));
                        let (dot_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                        ui.painter()
                            .circle_filled(dot_rect.center(), 5.0, current_color);
                    });
                });

                ui.add_space(16.0);

                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("有効ステージ数 (1〜8)")
                                .size(11.5)
                                .color(TEXT_MUTED),
                        );
                        Self::help_badge(
                            ui,
                            "マウス底面のDPIボタンで循環切り替えできる段数です。よく使う段数だけに絞ると誤操作を防げます。",
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.settings.dpi_stage_count > 1,
                                Self::dark_button(" − ", Vec2::new(28.0, 26.0)),
                            )
                            .clicked()
                        {
                            self.settings.dpi_stage_count -= 1;
                            if self.settings.active_dpi_stage >= self.settings.dpi_stage_count {
                                self.settings.active_dpi_stage = self.settings.dpi_stage_count - 1;
                            }
                        }
                        ui.label(
                            RichText::new(format!("{} ステージ", self.settings.dpi_stage_count))
                                .strong()
                                .color(TEXT_PRIMARY),
                        );
                        if ui
                            .add_enabled(
                                self.settings.dpi_stage_count < 8,
                                Self::dark_button(" + ", Vec2::new(28.0, 26.0)),
                            )
                            .clicked()
                        {
                            self.settings.dpi_stage_count += 1;
                        }
                    });
                });
            });
        });

        ui.add_space(8.0);

        let two_cols = ui.available_width() >= 520.0;
        let count = self.settings.dpi_stage_count;

        if two_cols {
            for pair_start in (0..count).step_by(2) {
                ui.columns(2, |cols| {
                    self.render_single_dpi_card(&mut cols[0], pair_start);
                    if pair_start + 1 < count {
                        self.render_single_dpi_card(&mut cols[1], pair_start + 1);
                    }
                });
                ui.add_space(6.0);
            }
        } else {
            for idx in 0..count {
                self.render_single_dpi_card(ui, idx);
                ui.add_space(6.0);
            }
        }
    }

    fn render_single_dpi_card(&mut self, ui: &mut egui::Ui, idx: usize) {
        let is_active = self.settings.active_dpi_stage == idx;
        let rgb_u8 = self.settings.dpi_stages[idx].color;
        let stage_color = Color32::from_rgb(rgb_u8[0], rgb_u8[1], rgb_u8[2]);

        Self::selectable_card(ui, is_active, ACCENT, |ui| {
            ui.horizontal(|ui| {
                let stage_btn = egui::Button::new(
                    RichText::new(format!("Stage {}", idx + 1))
                        .size(12.0)
                        .strong()
                        .color(if is_active {
                            Color32::WHITE
                        } else {
                            TEXT_PRIMARY
                        }),
                )
                .fill(if is_active { ACCENT_DARK } else { BG_BTN })
                .stroke(Stroke::new(1.0, if is_active { ACCENT } else { BORDER_CARD }));
                if ui.add(stage_btn).clicked() {
                    self.settings.active_dpi_stage = idx;
                }

                let mut rgb = [
                    rgb_u8[0] as f32 / 255.0,
                    rgb_u8[1] as f32 / 255.0,
                    rgb_u8[2] as f32 / 255.0,
                ];
                if ui
                    .color_edit_button_rgb(&mut rgb)
                    .on_hover_text("このDPIステージのLEDインジケーター色")
                    .changed()
                {
                    self.settings.dpi_stages[idx].color = [
                        (rgb[0] * 255.0) as u8,
                        (rgb[1] * 255.0) as u8,
                        (rgb[2] * 255.0) as u8,
                    ];
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let stage = &mut self.settings.dpi_stages[idx];
                    if ui
                        .add(Self::dark_button("+", Vec2::new(22.0, 22.0)))
                        .clicked()
                    {
                        stage.dpi = (stage.dpi + 50).min(30_000);
                    }
                    ui.label(
                        RichText::new(format!("{} DPI", stage.dpi))
                            .size(14.0)
                            .strong()
                            .color(if is_active { stage_color } else { TEXT_PRIMARY }),
                    );
                    if ui
                        .add(Self::dark_button("−", Vec2::new(22.0, 22.0)))
                        .clicked()
                    {
                        stage.dpi = stage.dpi.saturating_sub(50).max(50);
                    }
                });
            });

            ui.add_space(4.0);
            let stage = &mut self.settings.dpi_stages[idx];
            let mut dpi_u32 = stage.dpi as u32;
            ui.spacing_mut().slider_width = (ui.available_width() - 16.0).max(80.0);
            let slider = egui::Slider::new(&mut dpi_u32, 50..=30_000)
                .step_by(50.0)
                .show_value(false);
            if ui.add(slider).changed() {
                stage.dpi = ((dpi_u32 / 50) * 50).clamp(50, 30_000) as u16;
            }
        });
    }

    fn render_performance_tab(&mut self, ui: &mut egui::Ui) {
        let two_cols = ui.available_width() >= 540.0;
        if two_cols {
            ui.columns(2, |cols| {
                self.render_perf_left_cards(&mut cols[0]);
                self.render_perf_right_cards(&mut cols[1]);
            });
        } else {
            self.render_perf_left_cards(ui);
            ui.add_space(8.0);
            self.render_perf_right_cards(ui);
        }
    }

    fn render_perf_left_cards(&mut self, ui: &mut egui::Ui) {
        let max_hz = self
            .manager
            .info
            .as_ref()
            .map(|i| i.max_polling_hz)
            .unwrap_or(8000);

        // Polling rate card
        Self::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("ポーリングレート")
                        .strong()
                        .size(14.0)
                        .color(TEXT_PRIMARY),
                );
                Self::help_badge(
                    ui,
                    "1秒間にマウスからPCへ座標を送信する回数です。\n高いほど遅延が減り視点移動が滑らかになりますが、2K〜8K HzではCPU負荷とバッテリー消費が大きく増えます。ゲームがカクつく場合は1K〜2K Hzに下げてください。",
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} Hz", self.settings.polling_rate.hz()))
                            .strong()
                            .color(ACCENT),
                    );
                });
            });
            ui.label(
                RichText::new(format!("現在の接続上限: 最大 {max_hz} Hz"))
                    .size(11.0)
                    .color(TEXT_MUTED),
            );
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                for rate in PollingRate::ALL {
                    let allowed = rate.hz() <= max_hz;
                    let selected = self.settings.polling_rate == rate;
                    let label = if rate.hz() >= 1000 {
                        format!("{}K Hz", rate.hz() / 1000)
                    } else {
                        format!("{} Hz", rate.hz())
                    };
                    let btn = egui::Button::new(RichText::new(label).strong().color(
                        if selected {
                            Color32::WHITE
                        } else if allowed {
                            TEXT_PRIMARY
                        } else {
                            TEXT_MUTED
                        },
                    ))
                    .fill(if selected { ACCENT_DARK } else { BG_BTN })
                    .stroke(if selected {
                        Stroke::new(1.3, ACCENT)
                    } else {
                        Stroke::new(1.0, BORDER_CARD)
                    })
                    .min_size(Vec2::new(68.0, 30.0));

                    if ui.add_enabled(allowed, btn).clicked() {
                        self.settings.polling_rate = rate;
                    }
                }
            });
        });

        ui.add_space(8.0);

        // LOD, Debounce & Sleep card
        Self::card(ui, |ui| {
            ui.label(
                RichText::new("LOD・デバウンス・スリープ")
                    .strong()
                    .size(14.0)
                    .color(TEXT_PRIMARY),
            );
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("リフトオフディスタンス (LOD)")
                        .size(11.5)
                        .color(TEXT_MUTED),
                );
                Self::help_badge(
                    ui,
                    "マウスを持ち上げたときにセンサーの読み取りが止まる高さです。\n・0.7 mm: マウスを頻繁に持ち上げるローセンシ向け\n・1.0〜2.0 mm: マウスパッドとの相性で反応が途切れる場合や安定重視向け",
                );
            });
            ui.columns(3, |cols| {
                for (i, lod) in LiftOffDistance::ALL.iter().enumerate() {
                    let selected = self.settings.lod == *lod;
                    let btn = egui::Button::new(
                        RichText::new(lod.label())
                            .strong()
                            .color(if selected { Color32::WHITE } else { TEXT_PRIMARY }),
                    )
                    .fill(if selected { ACCENT_DARK } else { BG_BTN })
                    .stroke(if selected {
                        Stroke::new(1.3, ACCENT)
                    } else {
                        Stroke::new(1.0, BORDER_CARD)
                    })
                    .min_size(Vec2::new(cols[i].available_width(), 28.0));
                    if cols[i].add(btn).clicked() {
                        self.settings.lod = *lod;
                    }
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("デバウンスタイム").size(12.0).color(TEXT_PRIMARY));
                Self::help_badge(
                    ui,
                    "クリック時のチャタリング（意図しない二重クリック）を防ぐ待機時間です。\n0 msでクリック遅延は最小になりますが、二重入力が出る場合は2〜4 ms以上へ上げてください（光学スイッチ搭載機は0〜2 msでも安定しやすいです）。",
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} ms", self.settings.debounce_ms))
                            .strong()
                            .color(ACCENT),
                    );
                });
            });
            ui.spacing_mut().slider_width = (ui.available_width() - 16.0).max(80.0);
            ui.add(egui::Slider::new(&mut self.settings.debounce_ms, 0..=30).show_value(false));

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("スリープ移行時間").size(12.0).color(TEXT_PRIMARY));
                Self::help_badge(
                    ui,
                    "操作がない状態が続いたときに省電力スリープへ移行するまでの時間です。\n短いほどバッテリーが長持ちしますが、スリープ復帰時に一瞬だけ反応が遅れることがあります。",
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let secs = self.settings.sleep_time_sec;
                    let text = if secs >= 60 {
                        format!("{}分 {}秒", secs / 60, secs % 60)
                    } else {
                        format!("{secs}秒")
                    };
                    ui.label(RichText::new(text).strong().color(ACCENT));
                });
            });
            ui.spacing_mut().slider_width = (ui.available_width() - 16.0).max(80.0);
            ui.add(
                egui::Slider::new(&mut self.settings.sleep_time_sec, 10..=1800)
                    .step_by(10.0)
                    .show_value(false),
            );
        });
    }

    fn render_perf_right_cards(&mut self, ui: &mut egui::Ui) {
        // Sensor run mode card
        Self::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("PAW3950 センサーモード")
                        .strong()
                        .size(14.0)
                        .color(TEXT_PRIMARY),
                );
                Self::help_badge(
                    ui,
                    "センサーの動作電力とフレームレートを切り替えます。\n・LP (High-Speed): バッテリー重視\n・HP (Competitive): 競技向けの低遅延重視\n・20K FPS: 最高応答ですがバッテリー消費が最も多くなります",
                );
            });
            ui.add_space(6.0);

            let modes = [
                (
                    SensorRunMode::LowPower,
                    "High-Speed (LP)",
                    "バッテリー持ち重視の通常動作モード",
                ),
                (
                    SensorRunMode::HighPerformance,
                    "Competitive (HP)",
                    "競技向けの低遅延・高応答センサーモード",
                ),
                (
                    SensorRunMode::Scan20K,
                    "20K FPS Mode",
                    "最大フレームレートで追従する最高性能モード",
                ),
            ];

            for (mode, title, desc) in modes {
                let selected = self.settings.sensor_mode == mode;
                let frame_resp = egui::Frame::new()
                    .fill(if selected {
                        BG_CARD_ACTIVE
                    } else {
                        Color32::from_rgb(26, 32, 41)
                    })
                    .stroke(if selected {
                        Stroke::new(1.3, ACCENT)
                    } else {
                        Stroke::new(1.0, BORDER_CARD)
                    })
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(title)
                                    .strong()
                                    .color(if selected { ACCENT } else { TEXT_PRIMARY }),
                            );
                            if selected {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(RichText::new("選択中").size(10.5).color(ACCENT));
                                    },
                                );
                            }
                        });
                        ui.label(RichText::new(desc).size(11.0).color(TEXT_MUTED));
                    });
                if frame_resp.response.interact(Sense::click()).clicked() {
                    self.settings.sensor_mode = mode;
                }
                ui.add_space(4.0);
            }
        });

        ui.add_space(8.0);

        // Tracking & wireless toggles card
        Self::card(ui, |ui| {
            ui.label(
                RichText::new("トラッキング & 無線オプション")
                    .strong()
                    .size(14.0)
                    .color(TEXT_PRIMARY),
            );
            ui.add_space(6.0);

            Self::render_toggle_row(
                ui,
                "MotionSync",
                "センサー信号をUSBポーリングと同期",
                "センサーのデータ取得をUSBポーリング周期に同期させ、動きのムラを減らします。\n注意：有効にすると約0.5 ms程度のわずかな遅延が追加されます（最速応答重視ならOFF）。",
                &mut self.settings.motion_sync,
            );
            ui.separator();
            Self::render_toggle_row(
                ui,
                "Angle Snapping",
                "水平・垂直の直線移動を補正",
                "わずかな手のブレを吸収して水平・垂直に真っすぐ動かしやすくします。\n注意：FPSなどの精密エイムでは意図しない補正がかかるため、通常はOFF推奨です。",
                &mut self.settings.angle_snapping,
            );
            ui.separator();
            Self::render_toggle_row(
                ui,
                "Ripple Control",
                "高DPI時の細かなジッターを抑制",
                "高DPI設定時に出やすいカーソルの細かな震えを平滑化します。\n注意：わずかな入力遅延や操作感の変化が生じるため、通常DPI（400〜3200）ではOFF推奨です。",
                &mut self.settings.ripple_control,
            );
            ui.separator();
            Self::render_toggle_row(
                ui,
                "Long Range Mode",
                "無線出力を強化して通信安定性を向上",
                "2.4GHz無線の送信出力を上げて電波干渉や距離による途切れを防ぎます。\n注意：バッテリー消費が増えるため、ドングルが机上の近くにある場合はOFFでも問題ありません。",
                &mut self.settings.long_range_mode,
            );
        });
    }

    fn render_toggle_row(
        ui: &mut egui::Ui,
        title: &str,
        subtitle: &str,
        tooltip: &str,
        value: &mut bool,
    ) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(title)
                            .strong()
                            .size(12.5)
                            .color(TEXT_PRIMARY),
                    );
                    Self::help_badge(ui, tooltip);
                });
                ui.label(RichText::new(subtitle).size(10.5).color(TEXT_MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                Self::toggle_switch(ui, value);
            });
        });
    }

    fn render_profiles_tab(&mut self, ui: &mut egui::Ui) {
        let two_cols = ui.available_width() >= 520.0;
        if two_cols {
            ui.columns(2, |cols| {
                self.render_onboard_profiles_card(&mut cols[0]);
                self.render_json_io_card(&mut cols[1]);
            });
        } else {
            self.render_onboard_profiles_card(ui);
            ui.add_space(8.0);
            self.render_json_io_card(ui);
        }
    }

    fn render_onboard_profiles_card(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("オンボードプロファイル")
                        .strong()
                        .size(14.0)
                        .color(TEXT_PRIMARY),
                );
                Self::help_badge(
                    ui,
                    "マウス本体のメモリに保存されている最大4つのプロファイルを選択します。ゲーム用・作業用などで使い分けられます。",
                );
            });
            ui.label(
                RichText::new("マウス本体のメモリに保存されたプロファイルを選択します")
                    .size(11.0)
                    .color(TEXT_MUTED),
            );
            ui.add_space(8.0);

            for row in 0..2 {
                ui.columns(2, |cols| {
                    for col in 0..2 {
                        let p = (row * 2 + col) as u8;
                        let selected = self.settings.active_profile == p;
                        let btn = egui::Button::new(
                            RichText::new(format!("Profile {}", p + 1))
                                .size(13.5)
                                .strong()
                                .color(if selected {
                                    Color32::WHITE
                                } else {
                                    TEXT_PRIMARY
                                }),
                        )
                        .fill(if selected { ACCENT_DARK } else { BG_BTN })
                        .stroke(if selected {
                            Stroke::new(1.3, ACCENT)
                        } else {
                            Stroke::new(1.0, BORDER_CARD)
                        })
                        .min_size(Vec2::new(cols[col].available_width(), 44.0));
                        if cols[col].add(btn).clicked() {
                            self.settings.active_profile = p;
                        }
                    }
                });
                ui.add_space(6.0);
            }
        });
    }

    fn render_json_io_card(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("設定バックアップ (JSON)")
                        .strong()
                        .size(14.0)
                        .color(TEXT_PRIMARY),
                );
                Self::help_badge(
                    ui,
                    "現在のDPI・センサー設定をPC上のJSONファイルに保存・復元できます。設定を書き換える前の控えとして使ってください。",
                );
            });
            ui.label(
                RichText::new("現在のDPI・センサー設定をPC上のJSONファイルへ保存・復元します")
                    .size(11.0)
                    .color(TEXT_MUTED),
            );
            ui.add_space(8.0);
            ui.label(RichText::new("ファイルパス").size(11.5).color(TEXT_MUTED));
            ui.add(
                egui::TextEdit::singleline(&mut self.profile_path)
                    .desired_width(ui.available_width()),
            );
            ui.add_space(8.0);
            ui.columns(2, |cols| {
                if cols[0]
                    .add(Self::dark_button(
                        "JSONへ保存",
                        Vec2::new(cols[0].available_width(), 30.0),
                    ))
                    .clicked()
                {
                    match serde_json::to_string_pretty(&self.settings)
                        .map_err(|e| e.to_string())
                        .and_then(|s| fs::write(&self.profile_path, s).map_err(|e| e.to_string()))
                    {
                        Ok(()) => {
                            self.set_status(format!("設定を {} へ保存しました", self.profile_path), false)
                        }
                        Err(e) => self.set_status(format!("保存失敗: {e}"), true),
                    }
                }
                if cols[1]
                    .add(Self::dark_button(
                        "JSONから読込",
                        Vec2::new(cols[1].available_width(), 30.0),
                    ))
                    .clicked()
                {
                    match fs::read_to_string(&self.profile_path)
                        .map_err(|e| e.to_string())
                        .and_then(|s| {
                            serde_json::from_str::<MouseSettings>(&s).map_err(|e| e.to_string())
                        }) {
                        Ok(loaded) => {
                            self.settings = loaded;
                            self.set_status(
                                format!("{} から設定を読み込みました", self.profile_path),
                                false,
                            );
                        }
                        Err(e) => self.set_status(format!("読込失敗: {e}"), true),
                    }
                }
            });
        });
    }

    fn render_diagnostics_tab(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("疎通テスト:").strong().color(TEXT_PRIMARY));
                Self::help_badge(
                    ui,
                    "実機とのHID通信が正常かを確認する読み取り専用コマンドです。書き込みは一切行いません。",
                );
                if ui
                    .add_enabled(
                        self.manager.is_connected(),
                        Self::dark_button("0x1D ドングル照会", Vec2::new(0.0, 26.0)),
                    )
                    .clicked()
                {
                    match self.manager.query_dongle_version() {
                        Ok(v) => {
                            self.set_status(format!("ドングルFW: v{}.{}", v.major, v.minor), false)
                        }
                        Err(e) => self.set_status(format!("0x1D 失敗: {e}"), true),
                    }
                }
                if ui
                    .add_enabled(
                        self.manager.is_connected(),
                        Self::dark_button("0x01 ハンドシェイク", Vec2::new(0.0, 26.0)),
                    )
                    .clicked()
                {
                    match self.manager.query_handshake() {
                        Ok(h) => self.set_status(
                            format!(
                                "ハンドシェイク成功: MID={} / FW v{}.{}",
                                h.model_id, h.firmware_major, h.firmware_minor
                            ),
                            false,
                        ),
                        Err(e) => self.set_status(format!("0x01 失敗: {e}"), true),
                    }
                }
                if ui
                    .add(Self::dark_button("ログ消去", Vec2::new(0.0, 26.0)))
                    .clicked()
                {
                    self.manager.logs.clear();
                }
            });
        });

        ui.add_space(8.0);

        let two_cols = ui.available_width() >= 560.0;
        if two_cols {
            ui.columns(2, |cols| {
                self.render_diag_tools_column(&mut cols[0]);
                self.render_diag_log_column(&mut cols[1]);
            });
        } else {
            self.render_diag_tools_column(ui);
            ui.add_space(8.0);
            self.render_diag_log_column(ui);
        }
    }

    fn render_diag_tools_column(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("フラッシュダンプ (CMD 0x08 / 読取専用)")
                        .strong()
                        .color(TEXT_PRIMARY),
                );
                Self::help_badge(
                    ui,
                    "マウス本体のフラッシュメモリを読み出す安全な機能です。S-Centerで1項目だけ変更した前後を比べると、ファームウェア固有のアドレスを確認できます。",
                );
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("開始(Hex):").size(11.5).color(TEXT_MUTED));
                ui.add(egui::TextEdit::singleline(&mut self.dump_start_hex).desired_width(56.0));
                ui.label(RichText::new("長さ:").size(11.5).color(TEXT_MUTED));
                ui.add(egui::DragValue::new(&mut self.dump_length).range(8..=256));
                if ui
                    .add_enabled(
                        self.manager.is_connected(),
                        Self::dark_button("ダンプ実行", Vec2::new(0.0, 24.0)),
                    )
                    .clicked()
                {
                    match u16::from_str_radix(self.dump_start_hex.trim(), 16) {
                        Ok(start) => match self.manager.dump_flash_range(start, self.dump_length) {
                            Ok(bytes) => {
                                let mut lines = Vec::new();
                                for (i, chunk) in bytes.chunks(16).enumerate() {
                                    lines.push(format!(
                                        "{:04X}: {}",
                                        start + (i as u16) * 16,
                                        format_hex(chunk)
                                    ));
                                }
                                self.dump_output = lines.join("\n");
                                self.set_status("フラッシュダンプを取得しました", false);
                            }
                            Err(e) => self.set_status(format!("ダンプ失敗: {e}"), true),
                        },
                        Err(_) => self.set_status("開始アドレスは16進数で入力してください", true),
                    }
                }
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .id_salt("flash_dump_scroll")
                .max_height(155.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.dump_output)
                            .hint_text("ダンプ実行を押すとここに16進数データが表示されます")
                            .font(egui::TextStyle::Monospace)
                            .desired_width(ui.available_width()),
                    );
                });
        });

        ui.add_space(8.0);

        Self::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("カスタム17B送信 (0x55チェックサム自動)")
                        .strong()
                        .color(TEXT_PRIMARY),
                );
                Self::help_badge(
                    ui,
                    "開発・解析用の生パケット送信機能です。0x07（書き込み）コマンドは左下の書込許可がオンのときのみ送信できます。",
                );
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("CMD:").size(11.5).color(TEXT_MUTED));
                ui.add(egui::TextEdit::singleline(&mut self.raw_cmd_hex).desired_width(42.0));
                if ui
                    .add_enabled(
                        self.manager.is_connected(),
                        Self::dark_button("送信", Vec2::new(56.0, 24.0)),
                    )
                    .clicked()
                {
                    match (
                        u8::from_str_radix(self.raw_cmd_hex.trim(), 16),
                        parse_hex(&self.raw_payload_hex),
                    ) {
                        (Ok(cmd), Ok(payload)) => {
                            if cmd == 0x07 && !self.manager.write_unlocked && !self.manager.demo_mode
                            {
                                self.set_status("0x07 書き込みには書込許可が必要です", true);
                            } else {
                                let pkt = build_packet(cmd, &payload);
                                match self.manager.transact(&pkt) {
                                    Ok(resp) => self.set_status(
                                        format!("応答: {}", format_hex(&resp)),
                                        false,
                                    ),
                                    Err(e) => self.set_status(format!("送信失敗: {e}"), true),
                                }
                            }
                        }
                        _ => self.set_status("CMDまたはPayloadのHex書式が不正です", true),
                    }
                }
            });
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.raw_payload_hex)
                    .hint_text("Payload Hex (空白区切り最大14B 例: 00 00 00 08)")
                    .font(egui::TextStyle::Monospace)
                    .desired_width(ui.available_width()),
            );
        });
    }

    fn render_diag_log_column(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, |ui| {
            ui.label(RichText::new("HID 通信ログ").strong().color(TEXT_PRIMARY));
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("hid_log_scroll")
                .max_height(315.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if self.manager.logs.is_empty() {
                        ui.label(RichText::new("ログはまだありません").color(TEXT_MUTED));
                    }
                    for entry in &self.manager.logs {
                        let color = match entry.direction {
                            "TX" => ACCENT,
                            "RX" => GREEN,
                            "ERR" => RED,
                            _ => TEXT_MUTED,
                        };
                        let line = match &entry.raw_hex {
                            Some(hex) => format!("[{}] {}  {}", entry.direction, entry.summary, hex),
                            None => format!("[{}] {}", entry.direction, entry.summary),
                        };
                        ui.label(RichText::new(line).monospace().size(11.0).color(color));
                    }
                });
        });
    }
}

impl eframe::App for ScyroxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.manager.is_connected()
            && self.last_battery_poll.elapsed() >= Duration::from_secs(60)
        {
            self.last_battery_poll = Instant::now();
            let _ = self.manager.refresh_battery();
        }

        self.render_sidebar(ctx);

        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(BG_SIDEBAR)
                    .stroke(Stroke::new(1.0, BORDER_CARD))
                    .inner_margin(Margin::symmetric(12, 5)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let dot_color = if self.status_is_error { RED } else { ACCENT };
                    ui.label(RichText::new("●").size(9.0).color(dot_color));
                    ui.label(
                        RichText::new(&self.status_banner)
                            .size(11.5)
                            .color(if self.status_is_error { RED } else { TEXT_MUTED }),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(13, 17, 23))
                    .inner_margin(Margin::same(12)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.active_tab {
                    ActiveTab::Dpi => self.render_dpi_tab(ui),
                    ActiveTab::Performance => self.render_performance_tab(ui),
                    ActiveTab::Profiles => self.render_profiles_tab(ui),
                    ActiveTab::Diagnostics => self.render_diagnostics_tab(ui),
                });
            });
    }
}
