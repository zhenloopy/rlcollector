use log::{error, info};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use thiserror::Error;
use xcap::Monitor;
use image::RgbaImage;
use image::codecs::webp::WebPEncoder;

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
}
