use log::{debug, info, warn};
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// Manages an optional Ollama child process that we started ourselves.
pub struct OllamaProcess {
    child: Mutex<Option<Child>>,
}

impl OllamaProcess {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
        }
    }

    /// Find the Ollama binary. Checks `<app_data_dir>/ollama` first, then system PATH.
    pub fn find_binary(app_data_dir: &Path) -> Option<PathBuf> {
        let local_name = if cfg!(windows) {
            "ollama.exe"
        } else {
            "ollama"
        };

        // Check app data directory first
        let local_path = app_data_dir.join(local_name);
        if local_path.is_file() {
            info!("Found bundled Ollama binary at {}", local_path.display());
            return Some(local_path);
        }

        // Fall back to system PATH
        let which_cmd = if cfg!(windows) { "where" } else { "which" };
        match Command::new(which_cmd)
            .arg("ollama")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        {
            Ok(output) if output.status.success() => {
                let path_str = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !path_str.is_empty() {
                    let path = PathBuf::from(&path_str);
                    info!("Found system Ollama binary at {}", path.display());
                    return Some(path);
                }
            }
            _ => {}
        }

        warn!("Ollama binary not found in app data dir or system PATH");
        None
    }

    /// Start Ollama serve as a child process. Returns error if already running or spawn fails.
    pub fn start(&self, binary_path: &Path) -> Result<(), String> {
        let mut guard = self.child.lock().map_err(|e| e.to_string())?;

        // Check if we already have a running child
        if let Some(ref mut child) = *guard {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // Process exited, clear it
                    debug!("Previous Ollama process exited, starting new one");
                }
                Ok(None) => {
                    // Still running
                    return Ok(());
                }
                Err(e) => {
                    warn!("Error checking Ollama process status: {}", e);
                }
            }
        }

        info!("Starting Ollama serve from {}", binary_path.display());
        let child_proc = Command::new(binary_path)
            .arg("serve")
            .env("OLLAMA_HOST", "127.0.0.1:11434")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start Ollama: {}", e))?;

        info!("Ollama process started with PID {}", child_proc.id());
        *guard = Some(child_proc);
        Ok(())
    }

    /// Stop the managed Ollama process if we own one.
    pub fn stop(&self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                info!("Stopping managed Ollama process (PID {})", child.id());
                if let Err(e) = child.kill() {
                    // Process may have already exited
                    debug!("Kill returned error (may already be exited): {}", e);
                }
                let _ = child.wait();
            }
        }
    }

    /// Returns true if we started and still own a running Ollama process.
    pub fn is_managed(&self) -> bool {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = *guard {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        // Exited — clear it
                        *guard = None;
                        false
                    }
                    Ok(None) => true,
                    Err(_) => false,
                }
            } else {
                false
            }
        } else {
            false
        }
    }
}

impl Drop for OllamaProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Poll Ollama's API until it responds, or give up after `max_attempts` tries (500ms apart).
pub async fn wait_for_ready(client: &Client, max_attempts: u32) -> Result<(), String> {
    for attempt in 1..=max_attempts {
        match client.get("http://localhost:11434/api/tags").send().await {
            Ok(resp) if resp.status().is_success() => {
                info!("Ollama ready after {} attempt(s)", attempt);
                return Ok(());
            }
            Ok(resp) => {
                debug!(
                    "Ollama not ready (attempt {}/{}): HTTP {}",
                    attempt,
                    max_attempts,
                    resp.status()
                );
            }
            Err(e) => {
                debug!(
                    "Ollama not ready (attempt {}/{}): {}",
                    attempt, max_attempts, e
                );
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err(format!(
        "Ollama did not become ready after {} attempts",
        max_attempts
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_process_new() {
        let proc = OllamaProcess::new();
        assert!(!proc.is_managed());
    }

    #[test]
    fn test_find_binary_nonexistent_dir() {
        let result = OllamaProcess::find_binary(Path::new("/nonexistent/path"));
        // Should not find a bundled binary there; may or may not find system ollama
        // Just ensure it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_stop_when_not_running() {
        let proc = OllamaProcess::new();
        // Should not panic
        proc.stop();
        assert!(!proc.is_managed());
    }
}
