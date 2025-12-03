import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

function App() {
  const [progress, setProgress] = useState(0);
  const [status, setStatus] = useState("Idle");

  useEffect(() => {
    const unlistenPromise = listen('download_progress', (event: any) => {
      const { downloaded, total } = event.payload;
      if (total > 0) {
        setProgress((downloaded / total) * 100);
      }
    });
    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, []);

  async function download() {
    setStatus("Downloading...");
    try {
      // For demo, downloading a 100MB file to a temp path.
      // In production, use dialog.save() to pick path.
      // Hardcoding path for simplicity as per plan.
      await invoke("start_download", {
        url: "https://speed.hetzner.de/100MB.bin",
        path: "C:\\Users\\aditya\\Desktop\\test_download.bin"
      });
      setStatus("Done!");
    } catch (error) {
      console.error(error);
      setStatus("Error: " + error);
    }
  }

  return (
    <main className="container">
      <h1>HyperStream</h1>
      <div className="row">
        <button onClick={download}>Download 100MB Test File</button>
      </div>
      <div className="row">
        <p>Status: {status}</p>
        <progress value={progress} max="100" style={{ width: "100%" }}></progress>
        <p>{progress.toFixed(2)}%</p>
      </div>
    </main>
  );
}

export default App;
