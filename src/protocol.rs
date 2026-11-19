//! Compx / Nordic nRF52840 17-byte HID protocol implementation for Scyrox V6.
//!
//! References:
//! - https://github.com/sktdr-chan/sktdr-mouse
//! - https://github.com/Fan4Metal/mouse_tray/blob/master/mouse_tray/drivers/chipset/nordic52.py
//! - https://github.com/Olek97/Redragon_M913_Web_Configurator

use thiserror::Error;

pub const COMPX_VID: u16 = 0x3554;
pub const SCYROX_PID_8K_DONGLE: u16 = 0xF5F7;
pub const SCYROX_PID_WIRED: u16 = 0xF5F6;
pub const SCYROX_PID_STD_DONGLE: u16 = 0xF5F4;

pub const COMPX_USAGE_PAGE: u16 = 0xFF02;
pub const COMPX_USAGE: u16 = 0x0002;

pub const REPORT_ID: u8 = 0x08;
pub const REPORT_LEN: usize = 17;
pub const CHECKSUM_TARGET: u8 = 0x55;

/// Known Compx / Nordic52 command opcodes (byte[1]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    Handshake = 0x01,
    Battery = 0x04,
    WriteFlash = 0x07,
    ReadFlash = 0x08,
    DeviceNotify = 0x0A,
    DongleVersion = 0x1D,
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Invalid packet length: expected {expected} bytes, got {actual}")]
    InvalidLength { expected: usize, actual: usize },

    #[error("Invalid report ID: expected 0x08, got 0x{0:02X}")]
    InvalidReportId(u8),

    #[error("Checksum mismatch: expected 0x{expected:02X}, got 0x{actual:02X}")]
    ChecksumMismatch { expected: u8, actual: u8 },

    #[error("Flash payload too large: max 10 bytes per packet, got {0}")]
    PayloadTooLarge(usize),
}

/// Battery telemetry parsed from a Command::Battery (0x04) response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryStatus {
    pub status_byte: u8,
    pub percent: u8,
    pub wired_or_charging: bool,
    pub voltage_mv: u16,
}

/// Handshake response summary from Command::Handshake (0x01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeInfo {
    pub raw: [u8; REPORT_LEN],
    pub model_id: u8,
    pub firmware_major: u8,
    pub firmware_minor: u8,
}

/// Dongle version response summary from Command::DongleVersion (0x1D).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DongleVersionInfo {
    pub raw: [u8; REPORT_LEN],
    pub major: u8,
    pub minor: u8,
}

/// Calculates the Compx checksum byte (byte[16]) so that the sum of all 17 bytes
/// modulo 256 equals 0x55.
pub fn compute_checksum(packet_without_checksum: &[u8; REPORT_LEN - 1]) -> u8 {
    let sum: u8 = packet_without_checksum
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_add(b));
    CHECKSUM_TARGET.wrapping_sub(sum)
}

/// Verifies that a 17-byte packet has Report ID 0x08 and a valid 0x55 checksum.
pub fn verify_packet(packet: &[u8]) -> Result<[u8; REPORT_LEN], ProtocolError> {
    if packet.len() != REPORT_LEN {
        return Err(ProtocolError::InvalidLength {
            expected: REPORT_LEN,
            actual: packet.len(),
        });
    }
    if packet[0] != REPORT_ID {
        return Err(ProtocolError::InvalidReportId(packet[0]));
    }
    let mut prefix = [0u8; REPORT_LEN - 1];
    prefix.copy_from_slice(&packet[..REPORT_LEN - 1]);
    let expected = compute_checksum(&prefix);
    let actual = packet[REPORT_LEN - 1];
    if expected != actual {
        return Err(ProtocolError::ChecksumMismatch { expected, actual });
    }
    let mut out = [0u8; REPORT_LEN];
    out.copy_from_slice(packet);
    Ok(out)
}

/// Builds a 17-byte Compx HID report with Report ID 0x08 and trailing 0x55 checksum.
pub fn build_packet(command: u8, payload: &[u8]) -> [u8; REPORT_LEN] {
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    buf[1] = command;
    let copy_len = payload.len().min(REPORT_LEN - 3);
    buf[2..2 + copy_len].copy_from_slice(&payload[..copy_len]);

    let mut prefix = [0u8; REPORT_LEN - 1];
    prefix.copy_from_slice(&buf[..REPORT_LEN - 1]);
    buf[REPORT_LEN - 1] = compute_checksum(&prefix);
    buf
}

/// Builds a Handshake (0x01) request packet.
pub fn build_handshake_packet() -> [u8; REPORT_LEN] {
    build_packet(Command::Handshake as u8, &[])
}

/// Builds a Dongle Version (0x1D) query packet.
pub fn build_dongle_version_packet() -> [u8; REPORT_LEN] {
    build_packet(Command::DongleVersion as u8, &[])
}

/// Builds a Battery (0x04) query packet.
/// Verified wire format: `08 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 49`
pub fn build_battery_packet() -> [u8; REPORT_LEN] {
    build_packet(Command::Battery as u8, &[])
}

/// Builds a Read Flash (0x08) request packet for up to 10 bytes starting at `addr`.
/// Layout: `[0x08, 0x08, 0x00, addr_hi, addr_lo, len, 0..., checksum]`
pub fn build_read_flash_packet(addr: u16, len: u8) -> Result<[u8; REPORT_LEN], ProtocolError> {
    if len == 0 || len > 10 {
        return Err(ProtocolError::PayloadTooLarge(len as usize));
    }
    let payload = [0x00, (addr >> 8) as u8, (addr & 0xFF) as u8, len];
    Ok(build_packet(Command::ReadFlash as u8, &payload))
}

/// Builds a Write Flash (0x07) request packet for up to 10 bytes starting at `addr`.
/// Layout: `[0x08, 0x07, 0x00, addr_hi, addr_lo, len, data..., checksum]`
pub fn build_write_flash_packet(addr: u16, data: &[u8]) -> Result<[u8; REPORT_LEN], ProtocolError> {
    if data.is_empty() || data.len() > 10 {
        return Err(ProtocolError::PayloadTooLarge(data.len()));
    }
    let mut payload = [0u8; 14];
    payload[0] = 0x00;
    payload[1] = (addr >> 8) as u8;
    payload[2] = (addr & 0xFF) as u8;
    payload[3] = data.len() as u8;
    payload[4..4 + data.len()].copy_from_slice(data);
    Ok(build_packet(Command::WriteFlash as u8, &payload))
}

/// Parses a verified 17-byte Battery (0x04) response.
/// Verified layout from nordic52.py:
/// `08 04 00 00 00 <status> <percent> <wired> <v_hi> <v_lo> ... <cksum>`
pub fn parse_battery_response(packet: &[u8]) -> Result<BatteryStatus, ProtocolError> {
    let pkt = verify_packet(packet)?;
    let status_byte = pkt[5];
    let percent = pkt[6].min(100);
    let wired_or_charging = pkt[7] != 0;
    let voltage_mv = u16::from_be_bytes([pkt[8], pkt[9]]);
    Ok(BatteryStatus {
        status_byte,
        percent,
        wired_or_charging,
        voltage_mv,
    })
}

/// Parses a Handshake (0x01) response.
pub fn parse_handshake_response(packet: &[u8]) -> Result<HandshakeInfo, ProtocolError> {
    let pkt = verify_packet(packet)?;
    Ok(HandshakeInfo {
        raw: pkt,
        model_id: pkt[6],
        firmware_major: pkt[7],
        firmware_minor: pkt[8],
    })
}

/// Parses a Dongle Version (0x1D) response.
pub fn parse_dongle_version_response(packet: &[u8]) -> Result<DongleVersionInfo, ProtocolError> {
    let pkt = verify_packet(packet)?;
    Ok(DongleVersionInfo {
        raw: pkt,
        major: pkt[6],
        minor: pkt[7],
    })
}

/// Formats a byte slice as uppercase space-separated hex.
pub fn format_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parses a whitespace-separated hex string into bytes.
pub fn parse_hex(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for token in input.split_whitespace() {
        let cleaned = token.trim_start_matches("0x").trim_start_matches("0X");
        let byte = u8::from_str_radix(cleaned, 16)
            .map_err(|_| format!("Invalid hex byte: '{token}'"))?;
        out.push(byte);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_request_matches_verified_capture() {
        let pkt = build_battery_packet();
        let expected: [u8; 17] = [
            0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x49,
        ];
        assert_eq!(pkt, expected);
        assert!(verify_packet(&pkt).is_ok());
    }

    #[test]
    fn battery_response_matches_verified_capture() {
        let response: [u8; 17] = [
            0x08, 0x04, 0x00, 0x00, 0x00, 0x02, 0x23, 0x00, 0x0E, 0x94, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x82,
        ];
        let parsed = parse_battery_response(&response).expect("valid battery packet");
        assert_eq!(parsed.status_byte, 0x02);
        assert_eq!(parsed.percent, 35);
        assert!(!parsed.wired_or_charging);
        assert_eq!(parsed.voltage_mv, 3732);
    }

    #[test]
    fn read_and_write_flash_packets_have_valid_checksums() {
        let rd = build_read_flash_packet(0x0000, 10).unwrap();
        assert!(verify_packet(&rd).is_ok());
        assert_eq!(rd[1], Command::ReadFlash as u8);
        assert_eq!(rd[5], 10);

        let wr = build_write_flash_packet(0x000C, &[0x10, 0xEF]).unwrap();
        assert!(verify_packet(&wr).is_ok());
        assert_eq!(wr[1], Command::WriteFlash as u8);
        assert_eq!(wr[4], 0x0C);
        assert_eq!(wr[5], 2);
        assert_eq!(wr[6], 0x10);
        assert_eq!(wr[7], 0xEF);
    }
}
