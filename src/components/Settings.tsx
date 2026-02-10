import { useEffect, useState } from "react";
import { getSetting, updateSetting, getLogPath } from "../lib/tauri";
import { openPath } from "@tauri-apps/plugin-opener";

export function Settings() {
  const [apiKey, setApiKey] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    getSetting("ai_api_key").then((val) => {
      if (val) setApiKey(val);
    });
  }, []);

  const save = async () => {
    await updateSetting("ai_api_key", apiKey);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const openLogs = async () => {
    const logDir = await getLogPath();
    await openPath(logDir);
  };

  return (
    <div className="settings">
      <h2>Settings</h2>
      <label>
        Claude API Key:
        <input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="sk-ant-..."
        />
      </label>
      <button onClick={save}>Save</button>
      {saved && <span className="saved-msg">Saved</span>}
      <hr />
      <button onClick={openLogs}>Open Log Directory</button>
    </div>
  );
}
