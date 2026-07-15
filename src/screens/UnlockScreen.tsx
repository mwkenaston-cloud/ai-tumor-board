import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useApp } from "../app/AppContext";

/** Extract just the filename for display without leaking the full path in UI. */
function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export default function UnlockScreen() {
  const { actions, pendingOpenPath } = useApp();
  const [path, setPath] = useState<string | null>(pendingOpenPath);
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // A file handed to the app via double-click pre-selects here.
  useEffect(() => {
    if (pendingOpenPath) setPath(pendingOpenPath);
  }, [pendingOpenPath]);

  const pickFile = async () => {
    setError(null);
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "AI Tumor Board Assignment", extensions: ["atb"] }],
    });
    if (typeof selected === "string") setPath(selected);
  };

  const unlock = async () => {
    if (!path || !password) return;
    setBusy(true);
    setError(null);
    try {
      await actions.unlockAssignment(path, password);
    } catch (e) {
      // Backend returns a generic message; never reveal whether content exists.
      setError("Could not open the assignment. Check the password and try again.");
      console.error(e);
    } finally {
      setBusy(false);
    }
  };

  const loadDemo = async () => {
    setBusy(true);
    setError(null);
    try {
      await actions.useDemoAssignment();
    } catch (e) {
      setError("Could not load the demo assignment.");
      console.error(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="center-screen">
      <div className="card" style={{ maxWidth: 440 }}>
        <div style={{ fontSize: 34, textAlign: "center" }}>🔒</div>
        <h2 style={{ textAlign: "center" }}>Open assignment</h2>
        <p style={{ textAlign: "center" }}>
          Select your encrypted <code>.atb</code> assignment file and enter the password provided
          by the study coordinator.
        </p>

        <button className="btn btn-ghost" style={{ width: "100%", justifyContent: "center", marginBottom: 12 }} onClick={pickFile}>
          {path ? `📄 ${basename(path)}` : "Choose assignment file…"}
        </button>

        <label className="field-label">Password</label>
        <input
          className="text-input"
          type="password"
          value={password}
          autoFocus
          onChange={(e) => setPassword(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") unlock();
          }}
        />

        {error && <div className="form-error">{error}</div>}

        <button
          className="btn btn-primary"
          style={{ width: "100%", justifyContent: "center", marginTop: 16 }}
          disabled={!path || !password || busy}
          onClick={unlock}
        >
          {busy ? "Opening…" : "Unlock"}
        </button>

        <div style={{ textAlign: "center", marginTop: 16 }}>
          <button
            className="rec-detail-toggle"
            style={{ color: "var(--muted-2)" }}
            disabled={busy}
            onClick={loadDemo}
          >
            Load demo assignment (development)
          </button>
        </div>
      </div>
    </div>
  );
}
