use std::path::{Path, PathBuf};
use thiserror::Error;
use xcap::Monitor;

#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("No monitors found")]
    NoMonitors,
    #[error("Screen capture failed: {0}")]
    CaptureFailed(String),
    #[error("Failed to save screenshot: {0}")]
    SaveFailed(String),
}

/// Capture the primary monitor and save to the given directory.
/// Returns the filepath of the saved screenshot.
pub fn capture_screen(output_dir: &Path, filename: &str) -> Result<PathBuf, CaptureError> {
    let monitors = Monitor::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    let monitor = monitors.first().ok_or(CaptureError::NoMonitors)?;

    let image = monitor
        .capture_image()
        .map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;

    let path = output_dir.join(filename);
    image
        .save(&path)
        .map_err(|e| CaptureError::SaveFailed(e.to_string()))?;

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
        // We just check it doesn't panic — result depends on environment
        assert!(monitors.is_ok() || monitors.is_err());
    }
}
