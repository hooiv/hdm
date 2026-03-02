import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import App from "./App";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Overlay from "./Overlay";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ToastProvider } from "./contexts/ToastContext";

const windowLabel = getCurrentWindow().label;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <ToastProvider>
        {windowLabel === 'overlay' ? <Overlay /> : <App />}
      </ToastProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
