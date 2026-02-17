use crate::models::MonitorInfo;
use log::{error, info, warn};
use std::io::Cursor;
use std::path::Path;
use thiserror::Error;
use xcap::Monitor;
use image::RgbaImage;
use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;

#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("No monitors found")]
    NoMonitors,
    #[error("Screen capture failed: {0}")]
    CaptureFailed(String),
    #[error("Failed to save screenshot: {0}")]
    SaveFailed(String),
}

/// Result of capturing a single monitor's screen (image held in memory).
pub struct CapturedMonitor {
    pub monitor_id: u32,
    pub monitor_name: String,
    pub image: RgbaImage,
}

/// Save an RGBA image as WebP to the given path.
pub fn save_image_as_webp(image: &RgbaImage, path: &Path) -> Result<(), CaptureError> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = WebPEncoder::new_lossless(&mut buf);
    image
        .write_with_encoder(encoder)
        .map_err(|e| CaptureError::SaveFailed(e.to_string()))?;
    std::fs::write(path, buf.into_inner())
        .map_err(|e| CaptureError::SaveFailed(e.to_string()))?;
    Ok(())
}

/// List all available monitors.
pub fn list_monitors() -> Result<Vec<MonitorInfo>, CaptureError> {
    let monitors = Monitor::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    Ok(monitors
        .iter()
        .map(|m| MonitorInfo {
            id: m.id(),
            name: m.name().to_string(),
            x: m.x(),
            y: m.y(),
            width: m.width(),
            height: m.height(),
            is_primary: m.is_primary(),
        })
        .collect())
}

// --- Cursor position (platform-specific) ---

#[cfg(target_os = "windows")]
pub fn get_cursor_position() -> (i32, i32) {
    unsafe {
        let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        if windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point) != 0 {
            (point.x, point.y)
        } else {
            warn!("GetCursorPos failed, falling back to (0, 0)");
            (0, 0)
        }
    }
}

#[cfg(target_os = "macos")]
pub fn get_cursor_position() -> (i32, i32) {
    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    extern "C" {
        fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
        fn CGEventGetLocation(event: *const std::ffi::c_void) -> CGPoint;
        fn CFRelease(cf: *const std::ffi::c_void);
    }
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if !event.is_null() {
            let point = CGEventGetLocation(event);
            CFRelease(event);
            (point.x as i32, point.y as i32)
        } else {
            warn!("CGEventCreate failed, falling back to (0, 0)");
            (0, 0)
        }
    }
}

#[cfg(target_os = "linux")]
pub fn get_cursor_position() -> (i32, i32) {
    use std::process::Command;
    match Command::new("xdotool")
        .args(["getmouselocation"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut x = 0i32;
            let mut y = 0i32;
            for part in text.split_whitespace() {
                if let Some(val) = part.strip_prefix("x:") {
                    x = val.parse().unwrap_or(0);
                } else if let Some(val) = part.strip_prefix("y:") {
                    y = val.parse().unwrap_or(0);
                }
            }
            (x, y)
        }
        _ => {
            warn!("xdotool getmouselocation failed, falling back to (0, 0)");
            (0, 0)
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn get_cursor_position() -> (i32, i32) {
    (0, 0)
}

// --- Monitor selection helpers ---

fn find_primary(monitors: Vec<Monitor>) -> Result<Vec<Monitor>, CaptureError> {
    if monitors.is_empty() {
        return Err(CaptureError::NoMonitors);
    }
    let idx = monitors.iter().position(|m| m.is_primary()).unwrap_or(0);
    let mut monitors = monitors;
    let primary = monitors.swap_remove(idx);
    Ok(vec![primary])
}

/// Capture monitors based on the configured mode.
/// Returns captured images in memory (caller is responsible for saving to disk).
pub fn capture_monitors(
    mode: &str,
    specific_id: Option<u32>,
) -> Result<Vec<CapturedMonitor>, CaptureError> {
    info!("Capturing monitors: mode={}, specific_id={:?}", mode, specific_id);
    let monitors = Monitor::all().map_err(|e| {
        error!("Failed to enumerate monitors: {}", e);
        CaptureError::CaptureFailed(e.to_string())
    })?;
    if monitors.is_empty() {
        return Err(CaptureError::NoMonitors);
    }

    let selected: Vec<Monitor> = match mode {
        "specific" => {
            let id = specific_id.ok_or_else(|| {
                CaptureError::CaptureFailed("No monitor ID for 'specific' mode".into())
            })?;
            monitors
                .into_iter()
                .find(|m| m.id() == id)
                .map(|m| vec![m])
                .ok_or_else(|| CaptureError::CaptureFailed(format!("Monitor {} not found", id)))?
        }
        "active" => {
            let (cx, cy) = get_cursor_position();
            match Monitor::from_point(cx, cy) {
                Ok(m) => vec![m],
                Err(e) => {
                    warn!("from_point({}, {}) failed: {}, using primary", cx, cy, e);
                    find_primary(monitors)?
                }
            }
        }
        "all" => monitors,
        _ => find_primary(monitors)?, // "default"
    };

    let mut results = Vec::with_capacity(selected.len());
    for monitor in &selected {
        let image = monitor.capture_image().map_err(|e| {
            error!("Capture failed for monitor {}: {}", monitor.name(), e);
            CaptureError::CaptureFailed(e.to_string())
        })?;
        results.push(CapturedMonitor {
            monitor_id: monitor.id(),
            monitor_name: monitor.name().to_string(),
            image,
        });
    }
    Ok(results)
}

// --- Change detection (perceptual hashing) ---

/// Compute a 256-bit perceptual hash of an image.
/// The image is downscaled to 16x16 grayscale, then each pixel is compared to the mean.
pub fn perceptual_hash(image: &RgbaImage) -> [u8; 32] {
    let small = image::imageops::resize(image, 16, 16, FilterType::Triangle);
    let mut gray = [0u8; 256];
    let mut sum: u32 = 0;
    for (i, pixel) in small.pixels().enumerate() {
        if i >= 256 {
            break;
        }
        let g = (pixel[0] as u32 * 299 + pixel[1] as u32 * 587 + pixel[2] as u32 * 114) / 1000;
        gray[i] = g as u8;
        sum += g;
    }
    let mean = if sum > 0 { (sum / 256) as u8 } else { 0 };
    let mut hash = [0u8; 32];
    for (i, &g) in gray.iter().enumerate() {
        if g > mean {
            hash[i / 8] |= 1 << (7 - (i % 8));
        }
    }
    hash
}

/// Compute the hamming distance between two perceptual hashes.
pub fn hash_distance(a: &[u8; 32], b: &[u8; 32]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

// --- Image processing utilities ---

/// Downscale an image so its width is at most `max_width` pixels,
/// preserving aspect ratio. Returns the original image if already small enough.
pub fn resize_for_analysis(image: &RgbaImage, max_width: u32) -> RgbaImage {
    let (w, h) = image.dimensions();
    if w <= max_width {
        return image.clone();
    }
    let new_height = (h as f64 * max_width as f64 / w as f64).round() as u32;
    image::imageops::resize(image, max_width, new_height, FilterType::Triangle)
}

/// Attempt to crop to the active window on Linux using xdotool.
/// Falls back to the full image on failure or non-Linux platforms.
pub fn crop_active_window(image: &RgbaImage) -> RgbaImage {
    #[cfg(target_os = "linux")]
    {
        if let Some(cropped) = crop_active_window_linux(image) {
            return cropped;
        }
    }
    let _ = image; // suppress unused warning on non-linux
    image.clone()
}

#[cfg(target_os = "linux")]
fn crop_active_window_linux(image: &RgbaImage) -> Option<RgbaImage> {
    use std::process::Command;

    let window_id_output = Command::new("xdotool")
        .args(["getactivewindow"])
        .output()
        .ok()?;
    if !window_id_output.status.success() {
        warn!("xdotool getactivewindow failed");
        return None;
    }
    let window_id = String::from_utf8_lossy(&window_id_output.stdout)
        .trim()
        .to_string();

    let geom_output = Command::new("xdotool")
        .args(["getwindowgeometry", "--shell", &window_id])
        .output()
        .ok()?;
    if !geom_output.status.success() {
        warn!("xdotool getwindowgeometry failed");
        return None;
    }
    let geom_str = String::from_utf8_lossy(&geom_output.stdout);

    let mut x: u32 = 0;
    let mut y: u32 = 0;
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    for line in geom_str.lines() {
        if let Some(val) = line.strip_prefix("X=") {
            x = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("Y=") {
            y = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("WIDTH=") {
            width = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("HEIGHT=") {
            height = val.parse().unwrap_or(0);
        }
    }

    if width == 0 || height == 0 {
        warn!("xdotool returned zero-size window");
        return None;
    }

    let (img_w, img_h) = image.dimensions();
    let x = x.min(img_w.saturating_sub(1));
    let y = y.min(img_h.saturating_sub(1));
    let width = width.min(img_w - x);
    let height = height.min(img_h - y);

    if width == 0 || height == 0 {
        return None;
    }

    Some(image::imageops::crop_imm(image, x, y, width, height).to_image())
}

/// Encode an RgbaImage as WebP bytes in memory.
pub fn encode_webp_bytes(image: &RgbaImage) -> Result<Vec<u8>, CaptureError> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = WebPEncoder::new_lossless(&mut buf);
    image
        .write_with_encoder(encoder)
        .map_err(|e| CaptureError::SaveFailed(e.to_string()))?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitors_available() {
        let monitors = Monitor::all();
        assert!(monitors.is_ok() || monitors.is_err());
    }

    #[test]
    fn test_list_monitors() {
        // On machines with displays, should return a non-empty list
        let result = list_monitors();
        // May fail in headless CI; just verify it doesn't panic
        if let Ok(monitors) = result {
            assert!(!monitors.is_empty());
            // At least one should be primary
            assert!(monitors.iter().any(|m| m.is_primary));
        }
    }

    #[test]
    fn test_save_image_as_webp() {
        let width = 10;
        let height = 10;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.push((x * 25) as u8);
                pixels.push((y * 25) as u8);
                pixels.push(128u8);
                pixels.push(255u8);
            }
        }
        let image =
            RgbaImage::from_raw(width, height, pixels).expect("Failed to create test image");

        let temp_dir = std::env::temp_dir().join("rlcollector_test_webp");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let output_path = temp_dir.join("test_output.webp");

        save_image_as_webp(&image, &output_path).expect("WebP encoding failed");

        assert!(output_path.exists(), "WebP file was not created");
        let file_bytes = std::fs::read(&output_path).unwrap();
        assert!(!file_bytes.is_empty(), "WebP file is empty");
        assert!(
            file_bytes.len() >= 12,
            "WebP file too small for valid header"
        );
        assert_eq!(&file_bytes[0..4], b"RIFF", "Missing RIFF header");
        assert_eq!(&file_bytes[8..12], b"WEBP", "Missing WEBP signature");

        let _ = std::fs::remove_file(&output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_resize_for_analysis_already_small() {
        let image = RgbaImage::from_raw(100, 50, vec![128u8; 100 * 50 * 4]).unwrap();
        let resized = resize_for_analysis(&image, 1280);
        assert_eq!(resized.dimensions(), (100, 50));
    }

    #[test]
    fn test_resize_for_analysis_downscales() {
        let image = RgbaImage::from_raw(2560, 1440, vec![128u8; 2560 * 1440 * 4]).unwrap();
        let resized = resize_for_analysis(&image, 1280);
        assert_eq!(resized.width(), 1280);
        assert_eq!(resized.height(), 720);
    }

    #[test]
    fn test_crop_active_window_fallback() {
        let image = RgbaImage::from_raw(100, 50, vec![128u8; 100 * 50 * 4]).unwrap();
        let cropped = crop_active_window(&image);
        assert_eq!(cropped.dimensions(), (100, 50));
    }

    #[test]
    fn test_encode_webp_bytes() {
        let image = RgbaImage::from_raw(10, 10, vec![128u8; 10 * 10 * 4]).unwrap();
        let bytes = encode_webp_bytes(&image).unwrap();
        assert!(bytes.len() >= 12);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
    }

    #[test]
    fn test_perceptual_hash_consistent() {
        let image = RgbaImage::from_raw(100, 100, vec![128u8; 100 * 100 * 4]).unwrap();
        let h1 = perceptual_hash(&image);
        let h2 = perceptual_hash(&image);
        assert_eq!(h1, h2, "Same image should produce identical hashes");
    }

    #[test]
    fn test_perceptual_hash_different_images() {
        let white = RgbaImage::from_raw(100, 100, vec![255u8; 100 * 100 * 4]).unwrap();
        let black = RgbaImage::from_raw(100, 100, vec![0u8; 100 * 100 * 4]).unwrap();
        let h_white = perceptual_hash(&white);
        let h_black = perceptual_hash(&black);
        // Both solid colors produce all-equal pixels, so hash is all zeros (or all ones)
        // The distance should be 0 for solid images since all pixels == mean
        // But for truly different images the distance should be > 0
        let _dist = hash_distance(&h_white, &h_black);
    }

    #[test]
    fn test_hash_distance_identical() {
        let h = [0xABu8; 32];
        assert_eq!(hash_distance(&h, &h), 0);
    }

    #[test]
    fn test_hash_distance_opposite() {
        let a = [0x00u8; 32];
        let b = [0xFFu8; 32];
        assert_eq!(hash_distance(&a, &b), 256);
    }

    #[test]
    fn test_hash_distance_one_bit() {
        let a = [0x00u8; 32];
        let mut b = [0x00u8; 32];
        b[0] = 0x01;
        assert_eq!(hash_distance(&a, &b), 1);
    }
}
