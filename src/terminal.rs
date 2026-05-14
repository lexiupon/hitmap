//! Terminal geometry, Kitty Graphics Protocol encoding, and TTY I/O.

use base64::Engine as _;
use regex::bytes::Regex;
use std::io;
use std::io::IsTerminal;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const DEFAULT_CELL_WIDTH_PX: f64 = 10.0;
pub const DEFAULT_CELL_HEIGHT_PX: f64 = 20.0;
const PROBE_RESPONSE_PREVIEW_LIMIT: usize = 256;

/// Kitty graphics protocol query string.
const GRAPHICS_QUERY: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";

/// Primary device attributes query.
const PRIMARY_DEVICE_ATTRIBUTES_QUERY: &[u8] = b"\x1b[c";

/// OSC 11 background-color query.
const OSC_BACKGROUND_QUERY: &[u8] = b"\x1b]11;?\x1b\\";

/// Start of a Kitty graphics response.
const GRAPHICS_RESPONSE_START: &[u8] = b"\x1b_G";

/// End of a Kitty graphics response.
const GRAPHICS_RESPONSE_END: &[u8] = b"\x1b\\";

/// Regex for device attributes response.
static DEVICE_ATTRIBUTES_RESPONSE_RE: OnceLock<Regex> = OnceLock::new();

/// Regex for OSC 11 background-color response.
static OSC_BACKGROUND_RESPONSE_RE: OnceLock<Regex> = OnceLock::new();

fn get_device_attributes_re() -> &'static Regex {
    DEVICE_ATTRIBUTES_RESPONSE_RE.get_or_init(|| Regex::new(r"\x1b\[[?>0-9;:]*c").unwrap())
}

fn get_osc_background_re() -> &'static Regex {
    OSC_BACKGROUND_RESPONSE_RE.get_or_init(|| {
        Regex::new(
            r"\x1b\]11;rgb:([0-9A-Fa-f]{2,4})/([0-9A-Fa-f]{2,4})/([0-9A-Fa-f]{2,4})(?:\x07|\x1b\\)",
        )
        .unwrap()
    })
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Terminal geometry information.
#[derive(Debug, Clone)]
pub struct TerminalGeometry {
    pub rows: u16,
    pub cols: u16,
    pub width_px: u16,
    #[allow(dead_code)]
    pub height_px: u16,
    pub cell_width_px: f64,
    pub cell_height_px: f64,
}

#[derive(Debug, Clone)]
pub struct KittyProbeResult {
    pub graphics_seen: bool,
    pub device_attributes_seen: bool,
    pub response_bytes: Vec<u8>,
    pub trace: Vec<String>,
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Write all bytes to a file descriptor.
fn write_all(fd: RawFd, data: &[u8]) -> io::Result<()> {
    let mut remaining = data;
    while !remaining.is_empty() {
        match unsafe {
            libc::write(
                fd as _,
                remaining.as_ptr() as *const libc::c_void,
                remaining.len(),
            )
        } {
            -1 => return Err(io::Error::last_os_error()),
            0 => break, // Should not happen for pipes/TTYs
            n => {
                remaining = &remaining[(n as usize)..];
            }
        }
    }
    Ok(())
}

/// Scan terminal buffer for Kitty graphics and device attributes responses.
fn scan_terminal_responses(buffer: &[u8]) -> (bool, bool) {
    let graphics_seen = buffer
        .windows(GRAPHICS_RESPONSE_START.len())
        .position(|window| window == GRAPHICS_RESPONSE_START)
        .and_then(|start| {
            buffer[start..]
                .windows(GRAPHICS_RESPONSE_END.len())
                .position(|window| window == GRAPHICS_RESPONSE_END)
        })
        .is_some();
    let device_attributes_seen = get_device_attributes_re().is_match(buffer);
    (graphics_seen, device_attributes_seen)
}

fn parse_hex_color_component_to_u8(component: &[u8]) -> Option<u8> {
    let text = std::str::from_utf8(component).ok()?;
    let digits = text.len();
    if !(2..=4).contains(&digits) {
        return None;
    }
    let value = u32::from_str_radix(text, 16).ok()?;
    let max_value = (1_u32 << (digits * 4)) - 1;
    Some(((value * 255 + max_value / 2) / max_value) as u8)
}

fn parse_osc_background_response(buffer: &[u8]) -> Option<(u8, u8, u8)> {
    let captures = get_osc_background_re().captures(buffer)?;
    Some((
        parse_hex_color_component_to_u8(captures.get(1)?.as_bytes())?,
        parse_hex_color_component_to_u8(captures.get(2)?.as_bytes())?,
        parse_hex_color_component_to_u8(captures.get(3)?.as_bytes())?,
    ))
}

/// Format probe response bytes as a human-readable preview.
pub fn format_probe_response_preview(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "no response bytes received".to_string();
    }

    let shown = &bytes[..bytes.len().min(PROBE_RESPONSE_PREVIEW_LIMIT)];
    let escaped = shown
        .iter()
        .map(|byte| match byte {
            b'\x1b' => "\\x1b".to_string(),
            b'\r' => "\\r".to_string(),
            b'\n' => "\\n".to_string(),
            b'\t' => "\\t".to_string(),
            0x20..=0x7e => (*byte as char).to_string(),
            _ => format!("\\x{:02x}", byte),
        })
        .collect::<String>();
    let hex = shown
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<Vec<_>>()
        .join(" ");

    if bytes.len() > shown.len() {
        format!(
            "escaped=`{}` hex=`{}` (truncated, {} bytes total)",
            escaped,
            hex,
            bytes.len()
        )
    } else {
        format!("escaped=`{}` hex=`{}`", escaped, hex)
    }
}

pub fn format_probe_trace(trace: &[String]) -> String {
    if trace.is_empty() {
        return "no probe trace recorded".to_string();
    }
    trace.join("\n")
}

// ---------------------------------------------------------------------------
// Terminal probing
// ---------------------------------------------------------------------------

/// Probe whether the active terminal supports the Kitty graphics protocol.
pub fn probe_kitty_graphics(fd: RawFd, timeout_seconds: f64) -> io::Result<KittyProbeResult> {
    let mut trace = vec![format!(
        "probe start: fd={} timeout={:.3}s",
        fd, timeout_seconds
    )];

    // Save old terminal attributes
    let mut old_termios: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd as _, &mut old_termios) } != 0 {
        return Err(io::Error::last_os_error());
    }
    trace.push(format!(
        "tcgetattr ok: iflag=0x{:x} oflag=0x{:x} cflag=0x{:x} lflag=0x{:x} vmin={} vtime={}",
        old_termios.c_iflag,
        old_termios.c_oflag,
        old_termios.c_cflag,
        old_termios.c_lflag,
        old_termios.c_cc[libc::VMIN],
        old_termios.c_cc[libc::VTIME],
    ));

    // Match Python's tty.setraw()/cfmakeraw behavior as closely as possible.
    let mut raw_termios = old_termios;
    unsafe {
        libc::cfmakeraw(&mut raw_termios);
    }
    trace.push(format!(
        "cfmakeraw: iflag=0x{:x} oflag=0x{:x} cflag=0x{:x} lflag=0x{:x} vmin={} vtime={}",
        raw_termios.c_iflag,
        raw_termios.c_oflag,
        raw_termios.c_cflag,
        raw_termios.c_lflag,
        raw_termios.c_cc[libc::VMIN],
        raw_termios.c_cc[libc::VTIME],
    ));

    let tcsetattr_result = unsafe { libc::tcsetattr(fd as _, libc::TCSANOW, &raw_termios) };
    if tcsetattr_result != 0 {
        trace.push(format!(
            "tcsetattr raw failed: {}",
            io::Error::last_os_error()
        ));
        return Err(io::Error::last_os_error());
    }
    trace.push("tcsetattr raw ok".to_string());

    let result = (|| -> io::Result<KittyProbeResult> {
        // Flush input buffer before sending the probe.
        let flush_before = unsafe { libc::tcflush(fd as _, libc::TCIFLUSH) };
        if flush_before != 0 {
            trace.push(format!(
                "tcflush before probe failed: {}",
                io::Error::last_os_error()
            ));
        } else {
            trace.push("tcflush before probe ok".to_string());
        }

        let mut query =
            Vec::with_capacity(GRAPHICS_QUERY.len() + PRIMARY_DEVICE_ATTRIBUTES_QUERY.len());
        query.extend_from_slice(GRAPHICS_QUERY);
        query.extend_from_slice(PRIMARY_DEVICE_ATTRIBUTES_QUERY);
        trace.push(format!(
            "query bytes: len={} {}",
            query.len(),
            format_probe_response_preview(&query),
        ));
        write_all(fd, &query)?;
        trace.push("write_all query ok".to_string());
        let drain_result = unsafe { libc::tcdrain(fd as _) };
        if drain_result != 0 {
            trace.push(format!("tcdrain failed: {}", io::Error::last_os_error()));
        } else {
            trace.push("tcdrain ok".to_string());
        }

        let mut buffer = Vec::new();
        let mut graphics_seen = false;
        let mut device_attributes_seen = false;
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs_f64(timeout_seconds);

        loop {
            let now = std::time::Instant::now();
            if now >= deadline {
                trace.push("deadline reached before poll".to_string());
                break;
            }

            let remaining = deadline - now;
            let timeout_us = remaining.as_micros() as i64;
            if timeout_us <= 0 {
                trace.push("timeout_us <= 0 before select".to_string());
                break;
            }

            let mut read_fds: libc::fd_set = unsafe { std::mem::zeroed() };
            let mut error_fds: libc::fd_set = unsafe { std::mem::zeroed() };
            unsafe {
                libc::FD_ZERO(&mut read_fds);
                libc::FD_ZERO(&mut error_fds);
                libc::FD_SET(fd, &mut read_fds);
                libc::FD_SET(fd, &mut error_fds);
            }

            let mut timeout = libc::timeval {
                tv_sec: (timeout_us / 1_000_000) as _,
                tv_usec: (timeout_us % 1_000_000) as _,
            };

            let select_result = unsafe {
                libc::select(
                    fd + 1,
                    &mut read_fds,
                    std::ptr::null_mut(),
                    &mut error_fds,
                    &mut timeout,
                )
            };
            let read_ready = unsafe { libc::FD_ISSET(fd, &read_fds) };
            let error_ready = unsafe { libc::FD_ISSET(fd, &error_fds) };
            trace.push(format!(
                "select(timeout_us={}) => result={} read_ready={} error_ready={}",
                timeout_us, select_result, read_ready, error_ready
            ));

            if select_result < 0 {
                trace.push(format!("select failed: {}", io::Error::last_os_error()));
                break;
            }

            if select_result == 0 {
                trace.push("select timed out".to_string());
                break;
            }

            if error_ready {
                trace.push("select marked fd exceptional".to_string());
                break;
            }

            if read_ready {
                let mut chunk = [0u8; 4096];
                match unsafe {
                    libc::read(
                        fd as _,
                        chunk.as_mut_ptr() as *mut libc::c_void,
                        chunk.len(),
                    )
                } {
                    -1 => {
                        trace.push(format!("read failed: {}", io::Error::last_os_error()));
                        break;
                    }
                    0 => {
                        trace.push("read returned EOF".to_string());
                        break;
                    }
                    n => {
                        let chunk_bytes = &chunk[..(n as usize)];
                        trace.push(format!(
                            "read {} bytes: {}",
                            n,
                            format_probe_response_preview(chunk_bytes),
                        ));
                        buffer.extend_from_slice(chunk_bytes);
                        let (gs, das) = scan_terminal_responses(&buffer);
                        graphics_seen = gs;
                        device_attributes_seen = das;
                        trace.push(format!(
                            "scan buffer: total_bytes={} graphics_reply={} da_reply={}",
                            buffer.len(),
                            graphics_seen,
                            device_attributes_seen
                        ));
                        if graphics_seen && das {
                            trace.push(
                                "probe complete: saw graphics and device-attributes replies"
                                    .to_string(),
                            );
                            break;
                        }
                    }
                }
            } else {
                trace.push("select woke without read readiness".to_string());
            }
        }

        // Clear any trailing terminal replies so they do not leak into the shell prompt.
        let flush_after = unsafe { libc::tcflush(fd as _, libc::TCIFLUSH) };
        if flush_after != 0 {
            trace.push(format!(
                "tcflush after probe failed: {}",
                io::Error::last_os_error()
            ));
        } else {
            trace.push("tcflush after probe ok".to_string());
        }
        trace.push(format!(
            "probe finish: graphics_reply={} da_reply={} response_bytes={}",
            graphics_seen,
            device_attributes_seen,
            buffer.len()
        ));

        Ok(KittyProbeResult {
            graphics_seen,
            device_attributes_seen,
            response_bytes: buffer,
            trace,
        })
    })();

    let restore_result = unsafe { libc::tcsetattr(fd as _, libc::TCSANOW, &old_termios) };
    match &result {
        Ok(probe_result) => {
            let mut probe_result = probe_result.clone();
            if restore_result != 0 {
                probe_result.trace.push(format!(
                    "tcsetattr restore failed: {}",
                    io::Error::last_os_error()
                ));
                return Ok(probe_result);
            }
            probe_result.trace.push("tcsetattr restore ok".to_string());
            Ok(probe_result)
        }
        Err(err) => {
            if restore_result != 0 {
                return Err(io::Error::new(
                    err.kind(),
                    format!(
                        "{}; tcsetattr restore failed: {}",
                        err,
                        io::Error::last_os_error()
                    ),
                ));
            }
            Err(io::Error::new(err.kind(), err.to_string()))
        }
    }
}

/// Query the terminal background color via OSC 11.
pub fn query_terminal_background_rgb(
    fd: RawFd,
    timeout_seconds: f64,
) -> io::Result<Option<(u8, u8, u8)>> {
    let mut old_termios: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd as _, &mut old_termios) } != 0 {
        return Err(io::Error::last_os_error());
    }

    let mut raw_termios = old_termios;
    unsafe {
        libc::cfmakeraw(&mut raw_termios);
    }
    if unsafe { libc::tcsetattr(fd as _, libc::TCSANOW, &raw_termios) } != 0 {
        return Err(io::Error::last_os_error());
    }

    let result = (|| -> io::Result<Option<(u8, u8, u8)>> {
        let _ = unsafe { libc::tcflush(fd as _, libc::TCIFLUSH) };
        write_all(fd, OSC_BACKGROUND_QUERY)?;
        let _ = unsafe { libc::tcdrain(fd as _) };

        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs_f64(timeout_seconds);
        let mut buffer = Vec::new();

        loop {
            if let Some(rgb) = parse_osc_background_response(&buffer) {
                let _ = unsafe { libc::tcflush(fd as _, libc::TCIFLUSH) };
                return Ok(Some(rgb));
            }

            let now = std::time::Instant::now();
            if now >= deadline {
                let _ = unsafe { libc::tcflush(fd as _, libc::TCIFLUSH) };
                return Ok(None);
            }

            let remaining = deadline - now;
            let timeout_us = remaining.as_micros() as i64;
            let mut readfds: libc::fd_set = unsafe { std::mem::zeroed() };
            unsafe {
                libc::FD_ZERO(&mut readfds);
                libc::FD_SET(fd, &mut readfds);
            }
            let mut timeout = libc::timeval {
                tv_sec: (timeout_us / 1_000_000) as _,
                tv_usec: (timeout_us % 1_000_000) as _,
            };

            let select_result = unsafe {
                libc::select(
                    fd + 1,
                    &mut readfds,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut timeout,
                )
            };

            if select_result < 0 {
                return Err(io::Error::last_os_error());
            }
            if select_result == 0 || unsafe { !libc::FD_ISSET(fd, &readfds) } {
                continue;
            }

            let mut chunk = [0u8; 256];
            let read_result = unsafe {
                libc::read(
                    fd as _,
                    chunk.as_mut_ptr() as *mut libc::c_void,
                    chunk.len(),
                )
            };
            if read_result < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err);
            }
            if read_result == 0 {
                return Ok(None);
            }
            buffer.extend_from_slice(&chunk[..read_result as usize]);
        }
    })();

    let restore_result = unsafe { libc::tcsetattr(fd as _, libc::TCSANOW, &old_termios) };
    match result {
        Ok(value) => {
            if restore_result != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "terminal background query succeeded but restore failed: {}",
                        io::Error::last_os_error()
                    ),
                ));
            }
            Ok(value)
        }
        Err(err) => {
            if restore_result != 0 {
                return Err(io::Error::new(
                    err.kind(),
                    format!(
                        "{}; tcsetattr restore failed: {}",
                        err,
                        io::Error::last_os_error()
                    ),
                ));
            }
            Err(err)
        }
    }
}

// ---------------------------------------------------------------------------
// Terminal geometry
// ---------------------------------------------------------------------------

/// Get terminal geometry (rows, cols, pixel dimensions).
pub fn get_terminal_geometry(fd: RawFd) -> TerminalGeometry {
    let mut rows: u16 = 24;
    let mut cols: u16 = 80;

    // Try TIOCGWINSZ first
    let mut ws: [u16; 4] = [0; 4];
    unsafe {
        let ret = libc::ioctl(fd as _, libc::TIOCGWINSZ, ws.as_mut_ptr());
        if ret == 0 {
            rows = ws[0];
            cols = ws[1];
        }
    }

    // Fallback to defaults
    if rows == 0 {
        rows = 24;
    }
    if cols == 0 {
        cols = 80;
    }

    let cell_width_px = if cols > 0 {
        if ws[2] > 0 {
            (ws[2] as f64) / (cols as f64)
        } else {
            DEFAULT_CELL_WIDTH_PX
        }
    } else {
        DEFAULT_CELL_WIDTH_PX
    };
    let cell_height_px = if rows > 0 {
        if ws[3] > 0 {
            (ws[3] as f64) / (rows as f64)
        } else {
            DEFAULT_CELL_HEIGHT_PX
        }
    } else {
        DEFAULT_CELL_HEIGHT_PX
    };

    TerminalGeometry {
        rows,
        cols,
        width_px: ws[2],
        height_px: ws[3],
        cell_width_px: cell_width_px.max(1.0),
        cell_height_px: cell_height_px.max(1.0),
    }
}

// ---------------------------------------------------------------------------
// Kitty Graphics Protocol encoding
// ---------------------------------------------------------------------------

/// Encode a PNG for transport with the Kitty graphics protocol.
pub fn encode_kitty_chunks(
    png_bytes: &[u8],
    placement_cols: u32,
    placement_rows: Option<u32>,
    cursor_no_move: bool,
    chunk_size: usize,
) -> Result<Vec<Vec<u8>>, String> {
    if chunk_size == 0 || chunk_size % 4 != 0 {
        return Err("chunk_size must be a positive multiple of 4".to_string());
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(png_bytes);
    let mut control_prefix = format!("a=T,t=d,f=100,c={}", placement_cols);
    if let Some(rows) = placement_rows {
        control_prefix.push_str(&format!(",r={}", rows));
    }
    if cursor_no_move {
        control_prefix.push_str(",C=1");
    }

    let mut chunks = Vec::new();
    for offset in (0..encoded.len()).step_by(chunk_size) {
        let payload_bytes =
            &encoded.as_bytes()[offset..std::cmp::min(offset + chunk_size, encoded.len())];
        let more = if offset + chunk_size < encoded.len() {
            1
        } else {
            0
        };

        let control = if offset == 0 {
            format!("{},m={};", control_prefix, more)
        } else {
            format!("m={};", more)
        };

        let chunk = format!(
            "\x1b_G{}{}\x1b\\",
            control,
            std::str::from_utf8(payload_bytes).unwrap_or_default()
        )
        .into_bytes();
        chunks.push(chunk);
    }

    if chunks.is_empty() {
        let empty_control = format!("{},m=0;", control_prefix);
        chunks.push(format!("\x1b_G{}\x1b\\", empty_control).into_bytes());
    }

    Ok(chunks)
}

/// Display a PNG using Kitty graphics and leave the prompt below the image.
pub fn display_png_via_kitty(fd: RawFd, png_bytes: &[u8], placement_cols: u32) -> io::Result<()> {
    write_all(fd, b"\r")?;

    let chunks = encode_kitty_chunks(
        png_bytes,
        placement_cols,
        None,  // placement_rows
        false, // cursor_no_move — allow cursor to move after image
        4096,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    for chunk in chunks {
        write_all(fd, &chunk)?;
    }

    write_all(fd, b"\r")?;

    // Drain output buffer
    unsafe {
        libc::tcdrain(fd as _);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Terminal I/O
// ---------------------------------------------------------------------------

/// Open the controlling TTY for direct terminal I/O during render.
pub fn open_render_tty() -> std::io::Result<RawFd> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "hitmap render requires an interactive terminal",
        ));
    }
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")?;
    // Dup the fd so it stays valid after the File is dropped
    let fd = unsafe { libc::dup(tty.as_raw_fd()) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(fd)
}

/// Resolve the maximum image width in terminal cells.
/// Uses 90% of the terminal's pixel width converted to column count.
pub fn resolve_width_cells(requested: Option<u32>, geometry: &TerminalGeometry) -> u32 {
    let hard_max = (geometry.cols.max(1) as u32).saturating_sub(1);
    if let Some(cells) = requested {
        return cells.min(hard_max).max(1);
    }
    if geometry.width_px > 0 {
        let max_cols = (geometry.width_px as f64 * 0.9 / geometry.cell_width_px) as u32;
        return max_cols.max(1).min(hard_max);
    }
    ((geometry.cols as f64 * 0.9) as u32).max(1).min(hard_max)
}

/// Resolve the target display width in pixels for the rendered image.
pub fn resolve_target_width_px(placement_cols: u32, geometry: &TerminalGeometry) -> u32 {
    let mut width = (placement_cols as f64 * geometry.cell_width_px).round() as u32;
    if geometry.width_px > 0 {
        width = width.min(geometry.width_px as u32);
    }
    width.max(1)
}

/// Resolve the final displayed image width in terminal cells.
pub fn resolve_placement_cols(
    display_width_px: u32,
    max_width_cells: u32,
    geometry: &TerminalGeometry,
) -> u32 {
    let cols = (display_width_px as f64 / geometry.cell_width_px).round() as u32;
    cols.max(1).min(max_width_cells)
}

/// Render a hitmap to the terminal via Kitty graphics protocol, or stdout as fallback.

#[cfg(test)]
mod tests {
    use super::{
        parse_hex_color_component_to_u8, parse_osc_background_response, scan_terminal_responses,
    };

    #[test]
    fn scan_terminal_responses_detects_kitty_and_device_attributes() {
        let buffer = b"\x1b_Gi=31;OK\x1b\\\x1b[?62;22;52c";
        let (graphics_seen, device_attributes_seen) = scan_terminal_responses(buffer);
        assert!(graphics_seen);
        assert!(device_attributes_seen);
    }

    #[test]
    fn scan_terminal_responses_ignores_incomplete_kitty_reply() {
        let buffer = b"\x1b_Gi=31;OK";
        let (graphics_seen, device_attributes_seen) = scan_terminal_responses(buffer);
        assert!(!graphics_seen);
        assert!(!device_attributes_seen);
    }

    #[test]
    fn parse_hex_color_component_scales_16bit_values() {
        assert_eq!(parse_hex_color_component_to_u8(b"0000"), Some(0));
        assert_eq!(parse_hex_color_component_to_u8(b"ffff"), Some(255));
        assert_eq!(parse_hex_color_component_to_u8(b"8080"), Some(128));
    }

    #[test]
    fn parse_osc_background_response_extracts_rgb_triplet() {
        let buffer = b"\x1b]11;rgb:1e1e/1e1e/2eff\x1b\\";
        assert_eq!(parse_osc_background_response(buffer), Some((30, 30, 47)));
    }
}
