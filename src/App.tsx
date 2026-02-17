import { useCallback, useState } from "react";
import { CaptureControls } from "./components/CaptureControls";
import { Dashboard } from "./components/Dashboard";
import { Settings } from "./components/Settings";
import "./App.css";

type Tab = "sessions" | "settings";

function App() {
  const [tab, setTab] = useState<Tab>("sessions");
  const [sessionVersion, setSessionVersion] = useState(0);

  const handleCaptureStop = useCallback(() => {
    setSessionVersion((v) => v + 1);
  }, []);

  return (
    <div className="app">
      <header className="app-header">
        <h1>RLCollector</h1>
        <nav>
          <button
            className={tab === "sessions" ? "active" : ""}
            onClick={() => setTab("sessions")}
          >
            Sessions
          </button>
          <button
            className={tab === "settings" ? "active" : ""}
            onClick={() => setTab("settings")}
          >
            Settings
          </button>
        </nav>
      </header>
      <main>
        <CaptureControls onStop={handleCaptureStop} />
        {tab === "sessions" && <Dashboard refreshTrigger={sessionVersion} />}
        {tab === "settings" && <Settings />}
      </main>
    </div>
  );
}

export default App;
