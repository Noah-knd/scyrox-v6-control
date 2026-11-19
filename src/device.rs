//! Windows HID device discovery and safe communication layer for Scyrox V6.

use crate::config_map::{
    decode_dpi_value, encode_dpi_value, DeviceProfileConfig, LiftOffDistance, MouseSettings,
    PollingRate, SensorRunMode,
};
use crate::protocol::{
    build_battery_packet, build_dongle_version_packet, build_handshake_packet,
    build_read_flash_packet, build_write_flash_packet, format_hex, parse_battery_response,
    parse_dongle_version_response, parse_handshake_response, verify_packet, BatteryStatus,
    DongleVersionInfo, HandshakeInfo, REPORT_LEN,
};
use hidapi::{HidApi, HidDevice};
use std::thread;
use std::time::Duration;
use thiserror::Error;

const READ_TIMEOUT_MS: i32 = 350;
const INTER_PACKET_DELAY_MS: u64 = 20;

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("HID API error: {0}")]
    Hid(#[from] hidapi::HidError),

    #[error("Protocol error: {0}")]
    Protocol(#[from] crate::protocol::ProtocolError),

    #[error("No compatible Scyrox V6 HID interface (VID 0x3554, Usage Page 0xFF02) found")]
    NotFound,

    #[error("Device did not respond within timeout (mouse may be asleep — move or click it)")]
    Timeout,

    #[error("Write protection is active. Unlock writes in the top bar before applying changes.")]
    WriteLocked,

    #[error("Device is not connected")]
    NotConnected,
}

#[derive(Debug, Clone)]
pub struct ConnectedDeviceInfo {
    pub vid: u16,
    pub pid: u16,
    pub product_name: String,
    pub connection_label: String,
    pub max_polling_hz: u16,
    pub path: String,
    pub handshake: Option<HandshakeInfo>,
    pub dongle_version: Option<DongleVersionInfo>,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub direction: &'static str,
    pub summary: String,
    pub raw_hex: Option<String>,
}

pub struct ScyroxDeviceManager {
    pub config: DeviceProfileConfig,
    pub demo_mode: bool,
    pub write_unlocked: bool,
    pub info: Option<ConnectedDeviceInfo>,
    pub battery: Option<BatteryStatus>,
    pub logs: Vec<LogEntry>,
    hid_device: Option<HidDevice>,
    demo_flash: [u8; 256],
}

impl ScyroxDeviceManager {
    pub fn new(config: DeviceProfileConfig) -> Self {
        let mut demo_flash = [0u8; 256];
        // Pre-populate realistic defaults in virtual demo flash
        demo_flash[0] = PollingRate::Hz1000.to_wire();
        demo_flash[2] = 5;
        demo_flash[4] = 1;
        demo_flash[10] = LiftOffDistance::Mm10.to_wire();
        Self {
            config,
            demo_mode: false,
            write_unlocked: false,
            info: None,
            battery: None,
            logs: Vec::new(),
            hid_device: None,
            demo_flash,
        }
    }

    pub fn push_log(&mut self, direction: &'static str, summary: impl Into<String>, raw: Option<&[u8]>) {
        self.logs.push(LogEntry {
            direction,
            summary: summary.into(),
            raw_hex: raw.map(format_hex),
        });
        if self.logs.len() > 200 {
            let excess = self.logs.len() - 200;
            self.logs.drain(0..excess);
        }
    }

    pub fn is_connected(&self) -> bool {
        self.demo_mode || self.hid_device.is_some()
    }

    pub fn enable_demo_mode(&mut self, enable: bool) {
        self.demo_mode = enable;
        self.hid_device = None;
        if enable {
            self.info = Some(ConnectedDeviceInfo {
                vid: self.config.usb.vid,
                pid: 0xF5F7,
                product_name: "Scyrox V6 (Virtual Demo)".to_string(),
                connection_label: "8K Wireless Dongle (Demo)".to_string(),
                max_polling_hz: 8000,
                path: "demo://scyrox-v6".to_string(),
                handshake: Some(HandshakeInfo {
                    raw: [0; REPORT_LEN],
                    model_id: self.config.usb.expected_mid,
                    firmware_major: 2,
                    firmware_minor: 20,
                }),
                dongle_version: Some(DongleVersionInfo {
                    raw: [0; REPORT_LEN],
                    major: 2,
                    minor: 16,
                }),
            });
            self.battery = Some(BatteryStatus {
                status_byte: 0x02,
                percent: 84,
                wired_or_charging: false,
                voltage_mv: 4050,
            });
            self.push_log("SYS", "デモモードを有効化しました（実機へのHID通信は無効）", None);
        } else {
            self.info = None;
            self.battery = None;
            self.push_log("SYS", "デモモードを終了しました", None);
        }
    }

    /// Scans USB HID devices on Windows and opens the Scyrox V6 vendor collection
    /// (VID 0x3554, Usage Page 0xFF02, Usage 0x0002).
    pub fn connect(&mut self) -> Result<(), DeviceError> {
        if self.demo_mode {
            return Ok(());
        }
        self.hid_device = None;
        self.info = None;
        self.battery = None;

        let api = HidApi::new()?;
        let target_vid = self.config.usb.vid;
        let target_up = self.config.usb.usage_page;
        let target_usage = self.config.usb.usage;

        let mut matched = None;
        for dev in api.device_list() {
            if dev.vendor_id() != target_vid {
                continue;
            }
            let pid_entry = self.config.match_pid(dev.product_id());
            if pid_entry.is_none() {
                continue;
            }
            if dev.usage_page() == target_up && dev.usage() == target_usage {
                matched = Some((
                    dev.path().to_owned(),
                    dev.vendor_id(),
                    dev.product_id(),
                    dev.product_string()
                        .unwrap_or("Scyrox V6")
                        .to_string(),
                    pid_entry.unwrap().label.clone(),
                    pid_entry.unwrap().max_polling_hz,
                ));
                break;
            }
        }

        let Some((path, vid, pid, product_name, connection_label, max_polling_hz)) = matched else {
            self.push_log(
                "ERR",
                "Scyrox V6 (VID 0x3554 / Usage Page 0xFF02) が見つかりませんでした",
                None,
            );
            return Err(DeviceError::NotFound);
        };

        let device = api.open_path(&path)?;
        self.hid_device = Some(device);

        let path_str = path.to_string_lossy().into_owned();
        self.push_log(
            "SYS",
            format!("デバイスをオープンしました: {product_name} ({vid:04X}:{pid:04X})"),
            None,
        );

        let dongle_version = self.query_dongle_version().ok();
        let handshake = self.query_handshake().ok();

        self.info = Some(ConnectedDeviceInfo {
            vid,
            pid,
            product_name,
            connection_label,
            max_polling_hz,
            path: path_str,
            handshake,
            dongle_version,
        });

        let _ = self.refresh_battery();
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.hid_device = None;
        self.info = None;
        self.battery = None;
        self.push_log("SYS", "デバイスを切断しました", None);
    }

    /// Sends a 17-byte HID output report and waits for the matching 17-byte input report.
    pub fn transact(&mut self, request: &[u8; REPORT_LEN]) -> Result<[u8; REPORT_LEN], DeviceError> {
        self.push_log("TX", format!("CMD 0x{:02X}", request[1]), Some(request));

        if self.demo_mode {
            let mut resp = *request;
            if request[1] == 0x04 {
                resp[5] = 0x02;
                resp[6] = 84;
                resp[7] = 0x00;
                resp[8] = 0x0F;
                resp[9] = 0xD2; // 4050 mV
            } else if request[1] == 0x08 {
                let addr = u16::from_be_bytes([request[3], request[4]]) as usize;
                let len = (request[5] as usize).min(10);
                for i in 0..len {
                    if addr + i < self.demo_flash.len() {
                        resp[6 + i] = self.demo_flash[addr + i];
                    }
                }
            } else if request[1] == 0x07 {
                let addr = u16::from_be_bytes([request[3], request[4]]) as usize;
                let len = (request[5] as usize).min(10);
                for i in 0..len {
                    if addr + i < self.demo_flash.len() {
                        self.demo_flash[addr + i] = request[6 + i];
                    }
                }
            }
            let mut prefix = [0u8; REPORT_LEN - 1];
            prefix.copy_from_slice(&resp[..REPORT_LEN - 1]);
            resp[REPORT_LEN - 1] = crate::protocol::compute_checksum(&prefix);
            self.push_log("RX", format!("DEMO CMD 0x{:02X}", resp[1]), Some(&resp));
            return Ok(resp);
        }

        {
            let dev = self.hid_device.as_ref().ok_or(DeviceError::NotConnected)?;
            dev.write(request)?;
        }
        thread::sleep(Duration::from_millis(INTER_PACKET_DELAY_MS));

        let mut buf = [0u8; REPORT_LEN];
        // Retry reading up to 4 times in case an unsolicited 0x0A notification arrives first.
        for _ in 0..4 {
            let n = {
                let dev = self.hid_device.as_ref().ok_or(DeviceError::NotConnected)?;
                dev.read_timeout(&mut buf, READ_TIMEOUT_MS)?
            };
            if n == 0 {
                continue;
            }
            if n == REPORT_LEN {
                self.push_log("RX", format!("CMD 0x{:02X} ({n}B)", buf[1]), Some(&buf));
                if buf[1] == request[1] {
                    return Ok(verify_packet(&buf)?);
                }
            }
        }
        Err(DeviceError::Timeout)
    }

    pub fn query_handshake(&mut self) -> Result<HandshakeInfo, DeviceError> {
        let req = build_handshake_packet();
        let resp = self.transact(&req)?;
        Ok(parse_handshake_response(&resp)?)
    }

    pub fn query_dongle_version(&mut self) -> Result<DongleVersionInfo, DeviceError> {
        let req = build_dongle_version_packet();
        let resp = self.transact(&req)?;
        Ok(parse_dongle_version_response(&resp)?)
    }

    pub fn refresh_battery(&mut self) -> Result<BatteryStatus, DeviceError> {
        let req = build_battery_packet();
        let resp = self.transact(&req)?;
        let status = parse_battery_response(&resp)?;
        self.battery = Some(status);
        Ok(status)
    }

    /// Reads `len` bytes (1..=10) from the device flash map via command 0x08.
    pub fn read_flash_chunk(&mut self, addr: u16, len: u8) -> Result<Vec<u8>, DeviceError> {
        let req = build_read_flash_packet(addr, len)?;
        let resp = self.transact(&req)?;
        Ok(resp[6..6 + len as usize].to_vec())
    }

    /// Dumps an arbitrary flash range in 8-byte chunks (read-only, always safe).
    pub fn dump_flash_range(&mut self, start_addr: u16, total_len: u16) -> Result<Vec<u8>, DeviceError> {
        let mut out = Vec::with_capacity(total_len as usize);
        let mut offset = 0u16;
        while offset < total_len {
            let chunk_len = (total_len - offset).min(8) as u8;
            let bytes = self.read_flash_chunk(start_addr + offset, chunk_len)?;
            out.extend_from_slice(&bytes);
            offset += chunk_len as u16;
        }
        Ok(out)
    }

    /// Writes up to 10 bytes to the device flash map via command 0x07.
    /// Blocked unless `write_unlocked` is explicitly enabled.
    pub fn write_flash_chunk(&mut self, addr: u16, data: &[u8]) -> Result<(), DeviceError> {
        if !self.write_unlocked && !self.demo_mode {
            return Err(DeviceError::WriteLocked);
        }
        let req = build_write_flash_packet(addr, data)?;
        let _ = self.transact(&req)?;
        Ok(())
    }

    /// Reads settings from the mouse flash using `config.flash_map`.
    pub fn read_settings(&mut self) -> Result<MouseSettings, DeviceError> {
        let mut settings = MouseSettings::default();
        let map = self.config.flash_map.clone();

        if let Ok(b) = self.read_flash_chunk(map.polling_rate_addr, 2) {
            if let Some(rate) = PollingRate::from_wire(b[0]) {
                settings.polling_rate = rate;
            }
        }
        if let Ok(b) = self.read_flash_chunk(map.dpi_stage_count_addr, 2) {
            if (1..=8).contains(&b[0]) {
                settings.dpi_stage_count = b[0] as usize;
            }
        }
        if let Ok(b) = self.read_flash_chunk(map.active_dpi_stage_addr, 2) {
            if (b[0] as usize) < settings.dpi_stages.len() {
                settings.active_dpi_stage = b[0] as usize;
            }
        }
        if let Ok(b) = self.read_flash_chunk(map.lod_addr, 2) {
            if let Some(lod) = LiftOffDistance::from_wire(b[0]) {
                settings.lod = lod;
            }
        }
        for idx in 0..settings.dpi_stages.len() {
            let addr = map.dpi_stages_base_addr + (idx as u16) * map.dpi_stage_stride;
            if let Ok(b) = self.read_flash_chunk(addr, 4) {
                if let Some(dpi) = decode_dpi_value(&b) {
                    settings.dpi_stages[idx].dpi = dpi;
                }
            }
        }
        if let Ok(b) = self.read_flash_chunk(map.debounce_ms_addr, 2) {
            settings.debounce_ms = b[0].min(30);
        }
        if let Ok(b) = self.read_flash_chunk(map.motion_sync_addr, 2) {
            settings.motion_sync = b[0] != 0;
        }
        if let Ok(b) = self.read_flash_chunk(map.angle_snap_addr, 2) {
            settings.angle_snapping = b[0] != 0;
        }
        if let Ok(b) = self.read_flash_chunk(map.ripple_control_addr, 2) {
            settings.ripple_control = b[0] != 0;
        }
        if let Ok(b) = self.read_flash_chunk(map.sensor_mode_addr, 2) {
            if let Some(mode) = SensorRunMode::from_wire(b[0]) {
                settings.sensor_mode = mode;
            }
        }
        if let Ok(b) = self.read_flash_chunk(map.long_range_addr, 2) {
            settings.long_range_mode = b[0] != 0;
        }

        self.push_log("SYS", "デバイスから設定を読み出しました", None);
        Ok(settings)
    }

    /// Writes `MouseSettings` to the mouse flash using `config.flash_map`.
    pub fn apply_settings(&mut self, settings: &MouseSettings) -> Result<(), DeviceError> {
        if !self.write_unlocked && !self.demo_mode {
            return Err(DeviceError::WriteLocked);
        }
        let map = self.config.flash_map.clone();

        let write_pair = |mgr: &mut Self, addr: u16, val: u8| -> Result<(), DeviceError> {
            let comp = 0x55u8.wrapping_sub(val);
            mgr.write_flash_chunk(addr, &[val, comp])
        };

        write_pair(self, map.polling_rate_addr, settings.polling_rate.to_wire())?;
        write_pair(self, map.dpi_stage_count_addr, settings.dpi_stage_count as u8)?;
        write_pair(self, map.active_dpi_stage_addr, settings.active_dpi_stage as u8)?;
        write_pair(self, map.lod_addr, settings.lod.to_wire())?;

        for (idx, stage) in settings.dpi_stages.iter().enumerate() {
            let addr = map.dpi_stages_base_addr + (idx as u16) * map.dpi_stage_stride;
            let encoded = encode_dpi_value(stage.dpi);
            self.write_flash_chunk(addr, &encoded)?;

            let c_addr = map.dpi_colors_base_addr + (idx as u16) * map.dpi_color_stride;
            let [r, g, b] = stage.color;
            let c_cksum = 0x55u8.wrapping_sub(r).wrapping_sub(g).wrapping_sub(b);
            self.write_flash_chunk(c_addr, &[r, g, b, c_cksum])?;
        }

        write_pair(self, map.debounce_ms_addr, settings.debounce_ms.min(30))?;
        write_pair(self, map.motion_sync_addr, u8::from(settings.motion_sync))?;
        write_pair(self, map.angle_snap_addr, u8::from(settings.angle_snapping))?;
        write_pair(self, map.ripple_control_addr, u8::from(settings.ripple_control))?;
        write_pair(self, map.sensor_mode_addr, settings.sensor_mode.to_wire())?;
        write_pair(self, map.long_range_addr, u8::from(settings.long_range_mode))?;
        write_pair(self, map.active_profile_addr, settings.active_profile.min(3))?;

        self.push_log("SYS", "設定をデバイスへ書き込みました", None);
        Ok(())
    }
}
