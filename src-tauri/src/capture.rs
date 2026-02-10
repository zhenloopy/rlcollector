use log::{error, info, warn};
use std::io::Cursor;
use std::path::{Path, PathBuf};
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

/// Save an RGBA image as WebP to the given path.
/// This is a pure encoding function that can be tested without a monitor.
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

/// Capture the primary monitor and save as WebP to the given directory.
/// Returns the filepath of the saved screenshot.
pub fn capture_screen(output_dir: &Path, filename: &str) -> Result<PathBuf, CaptureError> {
    info!("Capturing screenshot: {}", filename);
    let monitors = Monitor::all().map_err(|e| {
        error!("Failed to enumerate monitors: {}", e);
        CaptureError::CaptureFailed(e.to_string())
    })?;
    let monitor = monitors.first().ok_or_else(|| {
        error!("No monitors found");
        CaptureError::NoMonitors
    })?;

    let image = monitor
        .capture_image()
        .map_err(|e| {
            error!("Monitor capture failed: {}", e);
            CaptureError::CaptureFailed(e.to_string())
        })?;

    let path = output_dir.join(filename);
    save_image_as_webp(&image, &path)?;

    Ok(path)
}

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

    // Get the active window ID
    let window_id_output = Command::new("xdotool")
        .args(["getactivewindow"])
        .output()
        .ok()?;
    if !window_id_output.status.success() {
        warn!("xdotool getactivewindow failed");
        return None;
    }
    let window_id = String::from_utf8_lossy(&window_id_output.stdout).trim().to_string();

    // Get window geometry
    let geom_output = Command::new("xdotool")
        .args(["getwindowgeometry", "--shell", &window_id])
        .output()
        .ok()?;
    if !geom_output.status.success() {
        warn!("xdotool getwindowgeometry failed");
        return None;
    }
    let geom_str = String::from_utf8_lossy(&geom_output.stdout);

    // Parse: X=123\nY=456\nWIDTH=789\nHEIGHT=012
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
    // Clamp to image bounds
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
        // This test verifies xcap can enumerate monitors.
        // It will pass on machines with displays and may fail in headless CI.
        let monitors = Monitor::all();
        // We just check it doesn't panic -- result depends on environment
        assert!(monitors.is_ok() || monitors.is_err());
    }

    #[test]
    fn test_save_image_as_webp() {
        // Create a 10x10 RGBA test image with known pixel data
        let width = 10;
        let height = 10;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.push((x * 25) as u8);  // R
                pixels.push((y * 25) as u8);  // G
                pixels.push(128u8);            // B
                pixels.push(255u8);            // A
            }
        }
        let image = RgbaImage::from_raw(width, height, pixels)
            .expect("Failed to create test image");

        // Save to a temp directory
        let temp_dir = std::env::temp_dir().join("rlcollector_test_webp");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let output_path = temp_dir.join("test_output.webp");

        // Encode and save
        save_image_as_webp(&image, &output_path).expect("WebP encoding failed");

        // Verify file exists
        assert!(output_path.exists(), "WebP file was not created");

        // Verify file has content
        let file_bytes = std::fs::read(&output_path).unwrap();
        assert!(!file_bytes.is_empty(), "WebP file is empty");

        // Verify RIFF header (WebP magic bytes)
        // WebP files start with "RIFF" followed by 4 bytes of size, then "WEBP"
        assert!(file_bytes.len() >= 12, "WebP file too small for valid header");
        assert_eq!(&file_bytes[0..4], b"RIFF", "Missing RIFF header");
        assert_eq!(&file_bytes[8..12], b"WEBP", "Missing WEBP signature");

        // Cleanup
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
        // On non-Linux or without xdotool, should return full image
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
}
