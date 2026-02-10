import { useState } from "react";
import { CaptureControls } from "./components/CaptureControls";
import { Collections } from "./components/Collections";
import { Dashboard } from "./components/Dashboard";
import { Settings } from "./components/Settings";
import "./App.css";

type Tab = "dashboard" | "collections" | "settings";

function App() {
  const [tab, setTab] = useState<Tab>("dashboard");

  return (
    <div className="app">
      <header className="app-header">
        <h1>RLCollector</h1>
        <nav>
          <button
            className={tab === "dashboard" ? "active" : ""}
            onClick={() => setTab("dashboard")}
          >
            Dashboard
          </button>
          <button
            className={tab === "collections" ? "active" : ""}
            onClick={() => setTab("collections")}
          >
            Collections
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
        <CaptureControls />
        {tab === "dashboard" && <Dashboard />}
        {tab === "collections" && <Collections />}
        {tab === "settings" && <Settings />}
      </main>
    </div>
  );
}

export default App;
