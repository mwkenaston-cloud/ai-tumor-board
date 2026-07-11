import { useEffect, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useApp } from "../app/AppContext";
import {
  coordinatorIpc,
  isTauri,
  type CoordinatorSummary,
  type CoordPatient,
} from "../services/ipc";

export default function CoordinatorScreen() {
  const { actions } = useApp();
  const [unlocked, setUnlocked] = useState(false);
  const [pw, setPw] = useState("");
  const [pwErr, setPwErr] = useState(false);
  const [summary, setSummary] = useState<CoordinatorSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  const refresh = async () => {
    try {
      setSummary(await coordinatorIpc.summary());
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    if (!unlocked) return;
    if (!isTauri()) {
      setError("Coordinator mode requires the desktop app (npm run tauri dev).");
      return;
    }
    coordinatorIpc
      .openWorkspace()
      .then(setSummary)
      .catch((e) => {
        console.error(e);
        setError("Could not open the coordinator workspace. A study credential may be required.");
      });
  }, [unlocked]);

  // Client-side coordinator password gate (parity with the prototype's editor lock).
  const tryUnlock = () => {
    if (pw === "edit") {
      setPwErr(false);
      setUnlocked(true);
    } else {
      setPwErr(true);
    }
  };

  if (!unlocked) {
    return (
      <div className="center-screen">
        <div className="card" style={{ maxWidth: 400 }}>
          <div style={{ fontSize: 32, textAlign: "center" }}>🗂️</div>
          <h2 style={{ textAlign: "center" }}>Coordinator access</h2>
          <p style={{ textAlign: "center" }}>Enter the coordinator password to continue.</p>
          <input
            className="text-input"
            type="password"
            autoFocus
            value={pw}
            onChange={(e) => { setPw(e.currentTarget.value); setPwErr(false); }}
            onKeyDown={(e) => { if (e.key === "Enter") tryUnlock(); }}
          />
          {pwErr && <div className="form-error">Incorrect password.</div>}
          <div style={{ display: "flex", gap: 8, marginTop: 14 }}>
            <button className="btn btn-ghost" style={{ flex: 1, justifyContent: "center" }} onClick={actions.goHome}>← Home</button>
            <button className="btn btn-primary" style={{ flex: 1, justifyContent: "center" }} onClick={tryUnlock}>Unlock</button>
          </div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="center-screen">
        <div className="card" style={{ maxWidth: 460, textAlign: "center" }}>
          <div style={{ fontSize: 32 }}>🗂️</div>
          <h2>Coordinator</h2>
          <p>{error}</p>
          <button className="btn btn-ghost" onClick={actions.goHome}>← Home</button>
        </div>
      </div>
    );
  }

  if (!summary) {
    return (
      <div className="center-screen">
        <div className="card" style={{ textAlign: "center" }}>Opening workspace…</div>
      </div>
    );
  }

  const selectedPatient = summary.patients.find((p) => p.id === selected) ?? null;

  return (
    <div className="lobby">
      <div style={{ maxWidth: 1120, margin: "0 auto", width: "96%", padding: "28px 0" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 }}>
          <h2 style={{ margin: 0, color: "#1e293b" }}>{summary.studyTitle}</h2>
          <button className="btn btn-ghost btn-sm" onClick={actions.goHome}>← Home</button>
        </div>
        {!summary.provisioned && (
          <div
            style={{
              background: "#fffbeb",
              border: "1px solid #fde68a",
              color: "#92400e",
              borderRadius: 8,
              padding: "8px 12px",
              fontSize: 12,
              marginBottom: 16,
            }}
          >
            Development build — no study authority credential is provisioned, so coordinator actions
            are unrestricted. Production builds require a signed coordinator credential.
          </div>
        )}

        <div style={{ display: "grid", gridTemplateColumns: "1.4fr 1fr", gap: 24, alignItems: "start" }}>
          {/* Left: patients */}
          <div>
            <SectionTitle>Patients ({summary.patients.length})</SectionTitle>
            <AddPatientForm onAdded={refresh} />
            <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 12 }}>
              {summary.patients.map((p) => (
                <PatientRow
                  key={p.id}
                  patient={p}
                  active={p.id === selected}
                  onSelect={() => setSelected(p.id === selected ? null : p.id)}
                  onRemove={async () => {
                    await coordinatorIpc.removePatient(p.id);
                    if (selected === p.id) setSelected(null);
                    refresh();
                  }}
                />
              ))}
              {summary.patients.length === 0 && (
                <div style={{ fontSize: 12, color: "var(--muted-2)" }}>No patients yet.</div>
              )}
            </div>
            {selectedPatient && (
              <PatientDetail patient={selectedPatient} onChanged={refresh} />
            )}
          </div>

          {/* Right: build + import + results */}
          <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
            <BuildPackagePanel summary={summary} onBuilt={refresh} />
            <ImportResponsePanel onImported={refresh} resultsCount={summary.resultsCount} />
          </div>
        </div>
      </div>
    </div>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h3 style={{ fontSize: 13, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--muted)", margin: "0 0 10px" }}>
      {children}
    </h3>
  );
}

function AddPatientForm({ onAdded }: { onAdded: () => void }) {
  const [researchId, setResearchId] = useState("");
  const [modelId, setModelId] = useState("");
  const [busy, setBusy] = useState(false);

  const add = async () => {
    if (!researchId.trim() || !modelId.trim()) return;
    setBusy(true);
    try {
      await coordinatorIpc.addPatient(researchId, modelId);
      setResearchId(""); setModelId("");
      onAdded();
    } catch (e) {
      console.error(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 14 }}>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
        <input className="text-input" placeholder="Research ID" value={researchId} onChange={(e) => setResearchId(e.currentTarget.value)} />
        <input className="text-input" placeholder="Model ID" value={modelId} onChange={(e) => setModelId(e.currentTarget.value)} />
      </div>
      <div style={{ fontSize: 11, color: "var(--muted-2)", marginTop: 6 }}>
        Clinical question and patient context are filled in from the imported AI output.
      </div>
      <button className="btn btn-primary btn-sm" style={{ marginTop: 8 }} disabled={busy || !researchId.trim() || !modelId.trim()} onClick={add}>
        + Add patient
      </button>
    </div>
  );
}

function PatientRow({ patient, active, onSelect, onRemove }: { patient: CoordPatient; active: boolean; onSelect: () => void; onRemove: () => void }) {
  return (
    <div
      className="queue-row"
      style={active ? { borderColor: "var(--primary-bright)", boxShadow: "var(--shadow-md)" } : undefined}
      onClick={onSelect}
    >
      <span className="q-id">{patient.researchId ?? patient.id}</span>
      <span className="q-label">
        {patient.cancerType ?? patient.modelId ?? "—"}
        {patient.modelId && patient.cancerType && (
          <span style={{ fontSize: 10.5, color: "var(--muted-2)", marginLeft: 6 }}>({patient.modelId})</span>
        )}
      </span>
      <span style={{ fontSize: 10.5, color: "var(--muted)" }}>
        {patient.documentCount} doc{patient.documentCount === 1 ? "" : "s"} · {patient.recommendationCount} rec
      </span>
      <button
        className="nb-remove-btn"
        title="Remove patient"
        onClick={(e) => { e.stopPropagation(); if (confirm(`Remove patient ${patient.researchId ?? patient.id}?`)) onRemove(); }}
      >
        ✕
      </button>
    </div>
  );
}

function PatientDetail({ patient, onChanged }: { patient: CoordPatient; onChanged: () => void }) {
  const [docType, setDocType] = useState("notes");
  const [docText, setDocText] = useState("");
  const [llm, setLlm] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const addDoc = async () => {
    if (!docText.trim()) return;
    setErr(null); setMsg(null);
    try {
      await coordinatorIpc.addDocument(patient.id, docType, `${docType}.txt`, docText);
      setDocText("");
      setMsg("Document added.");
      onChanged();
    } catch (e) {
      setErr(String(e));
    }
  };

  const importLlm = async () => {
    if (!llm.trim()) return;
    setErr(null); setMsg(null);
    try {
      const n = await coordinatorIpc.importLlm(patient.id, llm);
      setMsg(`Imported ${n} recommendation${n === 1 ? "" : "s"}.`);
      onChanged();
    } catch (e) {
      setErr(String(e).replace(/^.*Error: /, ""));
    }
  };

  return (
    <div style={{ marginTop: 12, background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 14 }}>
      <div style={{ fontWeight: 600, color: "#1e293b", marginBottom: 8 }}>
        {patient.researchId ?? patient.id} · {patient.cancerType ?? patient.modelId ?? "—"}
      </div>

      <div className="field-label">Add source document</div>
      <div style={{ display: "flex", gap: 8, marginBottom: 6 }}>
        <select className="text-input" style={{ maxWidth: 130 }} value={docType} onChange={(e) => setDocType(e.currentTarget.value)}>
          <option value="notes">Notes</option>
          <option value="pathology">Pathology</option>
          <option value="imaging">Imaging</option>
          <option value="labs">Labs</option>
        </select>
        <button className="btn btn-ghost btn-sm" disabled={!docText.trim()} onClick={addDoc}>Add</button>
      </div>
      <textarea className="text-input" style={{ minHeight: 60, resize: "vertical", marginBottom: 12 }} placeholder="Paste de-identified document text…" value={docText} onChange={(e) => setDocText(e.currentTarget.value)} />

      <div className="field-label">Import AI output (JSON)</div>
      <textarea className="text-input" style={{ minHeight: 80, resize: "vertical", fontFamily: "var(--font-mono)", fontSize: 11 }} placeholder='{"session_metadata": {...}, "phase3_recommendations": [...]}' value={llm} onChange={(e) => setLlm(e.currentTarget.value)} />
      <button className="btn btn-primary btn-sm" style={{ marginTop: 8 }} disabled={!llm.trim()} onClick={importLlm}>Validate & import</button>

      {msg && <div style={{ color: "var(--success)", fontSize: 12, marginTop: 8 }}>{msg}</div>}
      {err && <div className="form-error">{err}</div>}
    </div>
  );
}

function BuildPackagePanel({ summary, onBuilt }: { summary: CoordinatorSummary; onBuilt: () => void }) {
  const [reviewerId, setReviewerId] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [receipt, setReceipt] = useState<{ sha256: string; assignmentId: string } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const toggle = (id: string) =>
    setPicked((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });

  const build = async () => {
    setErr(null); setReceipt(null);
    if (!reviewerId.trim() || !password || picked.size === 0) return;
    const destination = await saveDialog({
      defaultPath: `TumorBoard_${reviewerId}.atb`,
      filters: [{ name: "AI Tumor Board Assignment", extensions: ["atb"] }],
    });
    if (!destination) return;
    setBusy(true);
    try {
      const r = await coordinatorIpc.buildPackage(reviewerId, displayName, [...picked], password, destination);
      setReceipt({ sha256: r.sha256, assignmentId: r.assignmentId });
      onBuilt();
    } catch (e) {
      setErr(String(e).replace(/^.*failed validation: /, "Validation failed: "));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 14 }}>
      <SectionTitle>Build assignment package</SectionTitle>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
        <input className="text-input" placeholder="Reviewer ID" value={reviewerId} onChange={(e) => setReviewerId(e.currentTarget.value)} />
        <input className="text-input" placeholder="Reviewer name" value={displayName} onChange={(e) => setDisplayName(e.currentTarget.value)} />
      </div>
      <input className="text-input" type="password" style={{ marginTop: 8 }} placeholder="Assignment password" value={password} onChange={(e) => setPassword(e.currentTarget.value)} />

      <div style={{ marginTop: 10, fontSize: 12, color: "var(--muted)" }}>Assign patients:</div>
      <div style={{ display: "flex", flexDirection: "column", gap: 4, marginTop: 4, maxHeight: 160, overflowY: "auto" }}>
        {summary.patients.map((p) => (
          <label key={p.id} style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 12, cursor: "pointer" }}>
            <input type="checkbox" checked={picked.has(p.id)} onChange={() => toggle(p.id)} />
            {p.researchId ?? p.id} · {p.cancerType ?? p.modelId ?? "—"}
            {(p.documentCount === 0 || p.recommendationCount === 0) && (
              <span style={{ color: "var(--warning)", fontSize: 10.5 }}>
                (needs {p.documentCount === 0 ? "a document" : ""}{p.documentCount === 0 && p.recommendationCount === 0 ? " + " : ""}{p.recommendationCount === 0 ? "AI output" : ""})
              </span>
            )}
          </label>
        ))}
      </div>

      <button className="btn btn-success btn-sm" style={{ marginTop: 10 }} disabled={busy || !reviewerId.trim() || !password || picked.size === 0} onClick={build}>
        {busy ? "Building…" : "Build & save encrypted .atb"}
      </button>
      {receipt && (
        <div style={{ marginTop: 8, fontSize: 11, color: "var(--success)" }}>
          Package built ({receipt.assignmentId}).
          <div style={{ fontFamily: "var(--font-mono)", fontSize: 10, wordBreak: "break-all" }}>SHA-256: {receipt.sha256}</div>
        </div>
      )}
      {err && <div className="form-error">{err}</div>}
    </div>
  );
}

function ImportResponsePanel({ onImported, resultsCount }: { onImported: () => void; resultsCount: number }) {
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const importFile = async () => {
    setErr(null); setMsg(null);
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "AI Tumor Board Response", extensions: ["atbr"] }],
    });
    if (typeof selected !== "string") return;
    try {
      const s = await coordinatorIpc.importResponse(selected);
      setMsg(`Imported response from ${s.reviewerId} (${s.patientCount} patients).`);
      onImported();
    } catch (e) {
      const text = String(e);
      setErr(text.includes("already imported") ? "This response was already imported." : "Could not import this response file.");
    }
  };

  return (
    <div style={{ background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 14 }}>
      <SectionTitle>Responses ({resultsCount})</SectionTitle>
      <p style={{ fontSize: 12, color: "var(--muted)", margin: "0 0 10px" }}>
        Import a reviewer's encrypted <code>.atbr</code> response. Duplicates are rejected.
      </p>
      <button className="btn btn-primary btn-sm" onClick={importFile}>Import response file…</button>
      {msg && <div style={{ color: "var(--success)", fontSize: 12, marginTop: 8 }}>{msg}</div>}
      {err && <div className="form-error">{err}</div>}
    </div>
  );
}
