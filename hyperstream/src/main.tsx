import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import App from "./App";
import Overlay from "./Overlay";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ToastProvider } from "./contexts/ToastContext";

// Safely get window label — getCurrentWindow() depends on __TAURI_INTERNALS__
// which may not be available yet or in non-Tauri environments (dev browser).
let windowLabel = "main";
try {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  windowLabel = getCurrentWindow().label;
} catch {
  // Not running inside Tauri — default to main window
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <ToastProvider>
        {windowLabel === 'overlay' ? <Overlay /> : <App />}
      </ToastProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
