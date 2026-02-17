import { useEffect, useState } from "react";
import { getSetting, updateSetting, getLogPath, ensureOllama, checkOllama, ollamaPull, getMonitors, highlightMonitors } from "../lib/tauri";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import type { MonitorInfo, OllamaStatus } from "../types";

export function Settings() {
  const [provider, setProvider] = useState<"ollama" | "claude">("claude");
  const [apiKey, setApiKey] = useState("");
  const [ollamaModel, setOllamaModel] = useState("qwen3-vl:8b");
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus | null>(null);
  const [checkingOllama, setCheckingOllama] = useState(false);
  const [pullingModel, setPullingModel] = useState(false);
  const [imageMode, setImageMode] = useState<"downscale" | "active_window">("downscale");
  const [analysisMode, setAnalysisMode] = useState<"realtime" | "batch">("batch");
  const [batchSize, setBatchSize] = useState(10);
  const [monitorMode, setMonitorMode] = useState<"default" | "specific" | "active" | "all">("default");
  const [monitorId, setMonitorId] = useState<string>("");
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
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
    getSetting("analysis_mode").then((val) => {
      if (val === "realtime" || val === "batch") setAnalysisMode(val);
    });
    getSetting("batch_size").then((val) => {
      if (val) {
        const n = parseInt(val, 10);
        if (n >= 1 && n <= 100) setBatchSize(n);
      }
    });
    getSetting("capture_monitor_mode").then((val) => {
      if (val === "default" || val === "specific" || val === "active" || val === "all")
        setMonitorMode(val);
    });
    getSetting("capture_monitor_id").then((val) => {
      if (val) setMonitorId(val);
    });
    refreshMonitors();
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

  const refreshMonitors = async () => {
    try {
      const list = await getMonitors();
      setMonitors(list);
    } catch {
      setMonitors([]);
    }
  };

  const formatMonitorName = (m: MonitorInfo, index: number) => {
    // Strip Windows device path prefix (\\.\)
    let label = m.name.replace(/^\\\\\.\\/i, "");
    // Add a human-readable number prefix
    label = `Monitor ${index + 1}: ${label}`;
    // Add resolution
    label += ` (${m.width}x${m.height})`;
    if (m.is_primary) label += " — Primary";
    return label;
  };

  const save = async () => {
    await updateSetting("ai_provider", provider);
    await updateSetting("image_mode", imageMode);
    await updateSetting("analysis_mode", analysisMode);
    await updateSetting("batch_size", String(batchSize));
    await updateSetting("capture_monitor_mode", monitorMode);
    if (monitorMode === "specific" && monitorId) {
      await updateSetting("capture_monitor_id", monitorId);
    }
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
        <legend>Monitor</legend>
        <label className="radio-label">
          <input
            type="radio"
            name="monitor_mode"
            value="default"
            checked={monitorMode === "default"}
            onChange={() => { setMonitorMode("default"); highlightMonitors("default").catch(() => {}); }}
          />
          Default (primary monitor)
        </label>
        <label className="radio-label">
          <input
            type="radio"
            name="monitor_mode"
            value="active"
            checked={monitorMode === "active"}
            onChange={() => { setMonitorMode("active"); highlightMonitors("active").catch(() => {}); }}
          />
          Active (follows cursor)
        </label>
        <label className="radio-label">
          <input
            type="radio"
            name="monitor_mode"
            value="all"
            checked={monitorMode === "all"}
            onChange={() => { setMonitorMode("all"); highlightMonitors("all").catch(() => {}); }}
          />
          All monitors
        </label>
        <label className="radio-label">
          <input
            type="radio"
            name="monitor_mode"
            value="specific"
            checked={monitorMode === "specific"}
            onChange={() => setMonitorMode("specific")}
          />
          Specific monitor
        </label>
        {monitorMode === "specific" && (
          <div style={{ marginLeft: "1.5rem", marginTop: "0.5rem" }}>
            <select
              value={monitorId}
              onChange={(e) => { setMonitorId(e.target.value); if (e.target.value) highlightMonitors("specific", Number(e.target.value)).catch(() => {}); }}
            >
              <option value="">Select a monitor...</option>
              {monitors.map((m, i) => (
                <option key={m.id} value={String(m.id)}>
                  {formatMonitorName(m, i)}
                </option>
              ))}
            </select>
            <button className="check-button" onClick={refreshMonitors} style={{ marginLeft: "0.5rem" }}>
              Refresh
            </button>
          </div>
        )}
        {monitors.length > 0 && monitorMode !== "specific" && (
          <div style={{ fontSize: "0.85em", opacity: 0.7, marginTop: "0.25rem" }}>
            {monitors.map((m, i) => formatMonitorName(m, i)).join(" | ")}
          </div>
        )}
      </fieldset>

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

      <fieldset className="provider-selector">
        <legend>Analysis Mode</legend>
        <label className="radio-label">
          <input
            type="radio"
            name="analysis_mode"
            value="realtime"
            checked={analysisMode === "realtime"}
            onChange={() => setAnalysisMode("realtime")}
          />
          Real-time (analyze each screenshot immediately)
        </label>
        <label className="radio-label">
          <input
            type="radio"
            name="analysis_mode"
            value="batch"
            checked={analysisMode === "batch"}
            onChange={() => setAnalysisMode("batch")}
          />
          Batch
        </label>
        {analysisMode === "batch" && (
          <label>
            Batch size:
            <input
              type="number"
              min={1}
              max={100}
              value={batchSize}
              onChange={(e) => {
                const n = parseInt(e.target.value, 10);
                if (!isNaN(n)) setBatchSize(Math.max(1, Math.min(100, n)));
              }}
            />
          </label>
        )}
      </fieldset>

      <button onClick={save}>Save</button>
      {saved && <span className="saved-msg">Saved</span>}
      <hr />
      <button onClick={openLogs}>Open Log Directory</button>
    </div>
  );
}
