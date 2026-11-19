//! Configuration models and externalizable flash address map for Scyrox V6.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const DEFAULT_MAP_JSON: &str = include_str!("../config/scyrox_v6_map.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PidEntry {
    pub pid: u16,
    pub label: String,
    pub max_polling_hz: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbConfig {
    pub vid: u16,
    pub pids: Vec<PidEntry>,
    pub usage_page: u16,
    pub usage: u16,
    pub expected_mid: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashMap {
    pub polling_rate_addr: u16,
    pub dpi_stage_count_addr: u16,
    pub active_dpi_stage_addr: u16,
    pub lod_addr: u16,
    pub dpi_stages_base_addr: u16,
    pub dpi_stage_stride: u16,
    pub dpi_colors_base_addr: u16,
    pub dpi_color_stride: u16,
    pub debounce_ms_addr: u16,
    pub motion_sync_addr: u16,
    pub sleep_time_addr: u16,
    pub angle_snap_addr: u16,
    pub ripple_control_addr: u16,
    pub sensor_mode_addr: u16,
    pub long_range_addr: u16,
    pub active_profile_addr: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfileConfig {
    pub schema_version: u32,
    pub device_name: String,
    pub protocol: String,
    pub notes: Vec<String>,
    pub usb: UsbConfig,
    pub flash_map: FlashMap,
}

impl DeviceProfileConfig {
    /// Loads `config/scyrox_v6_map.json` beside the executable/working dir if present,
    /// falling back to the embedded default map.
    pub fn load_or_default() -> Self {
        let candidate = Path::new("config/scyrox_v6_map.json");
        if let Ok(text) = fs::read_to_string(candidate) {
            if let Ok(parsed) = serde_json::from_str::<Self>(&text) {
                return parsed;
            }
        }
        serde_json::from_str(DEFAULT_MAP_JSON).expect("embedded scyrox_v6_map.json must be valid")
    }

    pub fn match_pid(&self, pid: u16) -> Option<&PidEntry> {
        self.usb.pids.iter().find(|p| p.pid == pid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PollingRate {
    Hz125,
    Hz250,
    Hz500,
    Hz1000,
    Hz2000,
    Hz4000,
    Hz8000,
}

impl PollingRate {
    pub const ALL: [PollingRate; 7] = [
        PollingRate::Hz125,
        PollingRate::Hz250,
        PollingRate::Hz500,
        PollingRate::Hz1000,
        PollingRate::Hz2000,
        PollingRate::Hz4000,
        PollingRate::Hz8000,
    ];

    pub fn hz(self) -> u16 {
        match self {
            PollingRate::Hz125 => 125,
            PollingRate::Hz250 => 250,
            PollingRate::Hz500 => 500,
            PollingRate::Hz1000 => 1000,
            PollingRate::Hz2000 => 2000,
            PollingRate::Hz4000 => 4000,
            PollingRate::Hz8000 => 8000,
        }
    }

    /// Compx report rate encoding (standard Compx table).
    pub fn to_wire(self) -> u8 {
        match self {
            PollingRate::Hz125 => 0x08,
            PollingRate::Hz250 => 0x04,
            PollingRate::Hz500 => 0x02,
            PollingRate::Hz1000 => 0x01,
            PollingRate::Hz2000 => 0x10,
            PollingRate::Hz4000 => 0x20,
            PollingRate::Hz8000 => 0x40,
        }
    }

    pub fn from_wire(v: u8) -> Option<Self> {
        match v {
            0x08 => Some(PollingRate::Hz125),
            0x04 => Some(PollingRate::Hz250),
            0x02 => Some(PollingRate::Hz500),
            0x01 => Some(PollingRate::Hz1000),
            0x10 => Some(PollingRate::Hz2000),
            0x20 => Some(PollingRate::Hz4000),
            0x40 => Some(PollingRate::Hz8000),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiftOffDistance {
    Mm07,
    Mm10,
    Mm20,
}

impl LiftOffDistance {
    pub const ALL: [LiftOffDistance; 3] = [
        LiftOffDistance::Mm07,
        LiftOffDistance::Mm10,
        LiftOffDistance::Mm20,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LiftOffDistance::Mm07 => "0.7 mm",
            LiftOffDistance::Mm10 => "1.0 mm",
            LiftOffDistance::Mm20 => "2.0 mm",
        }
    }

    pub fn to_wire(self) -> u8 {
        match self {
            LiftOffDistance::Mm07 => 0x00,
            LiftOffDistance::Mm10 => 0x01,
            LiftOffDistance::Mm20 => 0x02,
        }
    }

    pub fn from_wire(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(LiftOffDistance::Mm07),
            0x01 => Some(LiftOffDistance::Mm10),
            0x02 => Some(LiftOffDistance::Mm20),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensorRunMode {
    LowPower,
    HighPerformance,
    Scan20K,
}

impl SensorRunMode {
    pub const ALL: [SensorRunMode; 3] = [
        SensorRunMode::LowPower,
        SensorRunMode::HighPerformance,
        SensorRunMode::Scan20K,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SensorRunMode::LowPower => "High-Speed (LP / 省電力)",
            SensorRunMode::HighPerformance => "Competitive (HP / 高性能)",
            SensorRunMode::Scan20K => "20K FPS Mode",
        }
    }

    pub fn to_wire(self) -> u8 {
        match self {
            SensorRunMode::LowPower => 0x00,
            SensorRunMode::HighPerformance => 0x01,
            SensorRunMode::Scan20K => 0x02,
        }
    }

    pub fn from_wire(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(SensorRunMode::LowPower),
            0x01 => Some(SensorRunMode::HighPerformance),
            0x02 => Some(SensorRunMode::Scan20K),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpiStage {
    pub dpi: u16,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseSettings {
    pub active_profile: u8,
    pub polling_rate: PollingRate,
    pub lod: LiftOffDistance,
    pub debounce_ms: u8,
    pub sensor_mode: SensorRunMode,
    pub motion_sync: bool,
    pub angle_snapping: bool,
    pub ripple_control: bool,
    pub long_range_mode: bool,
    pub sleep_time_sec: u16,
    pub active_dpi_stage: usize,
    pub dpi_stage_count: usize,
    pub dpi_stages: Vec<DpiStage>,
}

impl Default for MouseSettings {
    fn default() -> Self {
        Self {
            active_profile: 0,
            polling_rate: PollingRate::Hz1000,
            lod: LiftOffDistance::Mm10,
            debounce_ms: 2,
            sensor_mode: SensorRunMode::HighPerformance,
            motion_sync: false,
            angle_snapping: false,
            ripple_control: false,
            long_range_mode: false,
            sleep_time_sec: 120,
            active_dpi_stage: 1,
            dpi_stage_count: 5,
            dpi_stages: vec![
                DpiStage { dpi: 400, color: [255, 64, 64] },
                DpiStage { dpi: 800, color: [64, 160, 255] },
                DpiStage { dpi: 1600, color: [64, 220, 120] },
                DpiStage { dpi: 3200, color: [255, 200, 50] },
                DpiStage { dpi: 6400, color: [190, 90, 255] },
                DpiStage { dpi: 12800, color: [255, 130, 50] },
                DpiStage { dpi: 16000, color: [80, 230, 230] },
                DpiStage { dpi: 26000, color: [240, 240, 240] },
            ],
        }
    }
}

/// Encodes DPI (50..=30000, step 50) into Compx 2-byte value + complement pair.
pub fn encode_dpi_value(dpi: u16) -> [u8; 4] {
    let clamped = dpi.clamp(50, 30_000);
    let step = (clamped / 50).saturating_sub(1);
    let lo = (step & 0xFF) as u8;
    let hi = ((step >> 8) & 0xFF) as u8;
    // Compx stores byte + complement (0x55 - byte) for integrity on many fields.
    let cksum = 0x55u8.wrapping_sub(lo).wrapping_sub(hi);
    [lo, lo, hi, cksum]
}

/// Decodes a 4-byte Compx DPI slot back to DPI (50..=30000).
pub fn decode_dpi_value(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 3 {
        return None;
    }
    let step = ((bytes[2] as u16) << 8) | (bytes[0] as u16);
    let dpi = (step + 1).saturating_mul(50);
    if (50..=30_000).contains(&dpi) {
        Some(dpi)
    } else {
        None
    }
}
