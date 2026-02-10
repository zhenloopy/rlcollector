import { useEffect, useState } from "react";
import { getSetting, updateSetting, getLogPath, ensureOllama, checkOllama, ollamaPull } from "../lib/tauri";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import type { OllamaStatus } from "../types";

export function Settings() {
  const [provider, setProvider] = useState<"ollama" | "claude">("claude");
  const [apiKey, setApiKey] = useState("");
  const [ollamaModel, setOllamaModel] = useState("qwen3-vl:8b");
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus | null>(null);
  const [checkingOllama, setCheckingOllama] = useState(false);
  const [pullingModel, setPullingModel] = useState(false);
  const [imageMode, setImageMode] = useState<"downscale" | "active_window">("downscale");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    getSetting("ai_provider").then((val) => {
      if (val === "ollama" || val === "claude") setProvider(val);
    });
    getSetting("ai_api_key").then((val) => {
      if (val) setApiKey(val);
    });
    getSetting("ollama_model").then((val) => {
      if (val) setOllamaModel(val);
    });
    getSetting("image_mode").then((val) => {
      if (val === "downscale" || val === "active_window") setImageMode(val);
    });
  }, []);

  useEffect(() => {
    if (provider === "ollama") {
      startOllama();
    }
  }, [provider]);

  const startOllama = async () => {
    setCheckingOllama(true);
    try {
      const status = await ensureOllama();
      setOllamaStatus(status);
    } catch (e) {
      setOllamaStatus({ available: false, models: [], source: "" });
    }
    setCheckingOllama(false);
  };

  const refreshOllamaStatus = async () => {
    setCheckingOllama(true);
    try {
      const status = await checkOllama();
      setOllamaStatus(status);
    } catch {
      setOllamaStatus({ available: false, models: [], source: "" });
    }
    setCheckingOllama(false);
  };

  const handlePullModel = async () => {
    setPullingModel(true);
    try {
      await ollamaPull(ollamaModel);
      await refreshOllamaStatus();
    } catch (e) {
      console.error("Failed to pull model:", e);
    }
    setPullingModel(false);
  };

  const save = async () => {
    await updateSetting("ai_provider", provider);
    await updateSetting("image_mode", imageMode);
    if (provider === "claude") {
      await updateSetting("ai_api_key", apiKey);
    } else {
      await updateSetting("ollama_model", ollamaModel);
    }
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const openLogs = async () => {
    const logDir = await getLogPath();
    await openPath(logDir);
  };

  const modelAvailable =
    ollamaStatus?.available &&
    ollamaStatus.models.some((m) => m === ollamaModel || m.startsWith(ollamaModel + ":"));

  const sourceLabel =
    ollamaStatus?.source === "external"
      ? " (external Ollama)"
      : ollamaStatus?.source === "bundled"
        ? " (bundled)"
        : "";

  return (
    <div className="settings">
      <h2>Settings</h2>

      <fieldset className="provider-selector">
        <legend>AI Provider</legend>
        <label className="radio-label">
          <input
            type="radio"
            name="provider"
            value="ollama"
            checked={provider === "ollama"}
            onChange={() => setProvider("ollama")}
          />
          Local (Ollama)
        </label>
        <label className="radio-label">
          <input
            type="radio"
            name="provider"
            value="claude"
            checked={provider === "claude"}
            onChange={() => setProvider("claude")}
          />
          Cloud (Claude)
        </label>
      </fieldset>

      {provider === "ollama" && (
        <div className="provider-config">
          <label>
            Model:
            <input
              type="text"
              value={ollamaModel}
              onChange={(e) => setOllamaModel(e.target.value)}
              placeholder="qwen3-vl:8b"
            />
          </label>
          <div className="ollama-status">
            <span
              className={`indicator ${ollamaStatus?.available ? "active" : "inactive"}`}
            />
            {checkingOllama
              ? "Starting..."
              : ollamaStatus === null
                ? "Not checked"
                : ollamaStatus.available
                  ? modelAvailable
                    ? `Connected${sourceLabel} — model "${ollamaModel}" available`
                    : `Connected${sourceLabel} — model "${ollamaModel}" not found (available: ${ollamaStatus.models.join(", ") || "none"})`
                  : "Binary not found"}
            <button className="check-button" onClick={refreshOllamaStatus} disabled={checkingOllama}>
              Refresh
            </button>
          </div>
          {ollamaStatus?.available && !modelAvailable && (
            <button onClick={handlePullModel} disabled={pullingModel}>
              {pullingModel ? "Pulling..." : `Pull "${ollamaModel}"`}
            </button>
          )}
          {ollamaStatus !== null && !ollamaStatus.available && !checkingOllama && (
            <button onClick={() => openUrl("https://ollama.com/download")}>
              Download Ollama
            </button>
          )}
        </div>
      )}

      {provider === "claude" && (
        <div className="provider-config">
          <label>
            Claude API Key:
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-ant-..."
            />
          </label>
        </div>
      )}

      <fieldset className="provider-selector">
        <legend>Image Mode</legend>
        <label className="radio-label">
          <input
            type="radio"
            name="image_mode"
            value="downscale"
            checked={imageMode === "downscale"}
            onChange={() => setImageMode("downscale")}
          />
          Full screen (downscaled)
        </label>
        <label className="radio-label">
          <input
            type="radio"
            name="image_mode"
            value="active_window"
            checked={imageMode === "active_window"}
            onChange={() => setImageMode("active_window")}
          />
          Active window only
        </label>
      </fieldset>

      <button onClick={save}>Save</button>
      {saved && <span className="saved-msg">Saved</span>}
      <hr />
      <button onClick={openLogs}>Open Log Directory</button>
    </div>
  );
}
