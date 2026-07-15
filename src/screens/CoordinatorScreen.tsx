import { useEffect, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useApp } from "../app/AppContext";
import {
  coordinatorIpc,
  isTauri,
  type CoordinatorSummary,
  type CoordPatient,
  type ResponsesView,
  type Batch,
  type AggPatient,
  type ReviewerGrid,
} from "../services/ipc";

export default function CoordinatorScreen() {
  const { actions } = useApp();
  const [unlocked, setUnlocked] = useState(false);
  const [pw, setPw] = useState("");
  const [pwErr, setPwErr] = useState(false);
  const [summary, setSummary] = useState<CoordinatorSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [tab, setTab] = useState<"build" | "reviewers" | "responses">("build");

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
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 12 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <h2 style={{ margin: 0, color: "#1e293b" }}>{summary.studyTitle}</h2>
            <button className={`btn btn-sm ${tab === "build" ? "btn-primary" : "btn-ghost"}`} onClick={() => setTab("build")}>
              Build &amp; assign
            </button>
            <button className={`btn btn-sm ${tab === "reviewers" ? "btn-primary" : "btn-ghost"}`} onClick={() => setTab("reviewers")}>
              Reviewers
            </button>
            <button className={`btn btn-sm ${tab === "responses" ? "btn-primary" : "btn-ghost"}`} onClick={() => setTab("responses")}>
              Responses &amp; results
            </button>
          </div>
          <button className="btn btn-ghost btn-sm" onClick={actions.goHome}>← Home</button>
        </div>

        {tab === "build" ? (
          <div style={{ display: "grid", gridTemplateColumns: "1.2fr 1fr", gap: 24, alignItems: "start" }}>
            {/* Left: patient roster */}
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
            </div>
            {/* Right: selected patient details (documents + AI import) */}
            <div>
              {selectedPatient ? (
                <PatientDetail patient={selectedPatient} onChanged={refresh} />
              ) : (
                <div className="doc-empty">Select a patient to add source documents and import AI output.</div>
              )}
            </div>
          </div>
        ) : tab === "reviewers" ? (
          <ReviewersTab allPatients={summary.patients} />
        ) : (
          <ResponsesTab />
        )}
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
  const [llm, setLlm] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const uploadCombined = async () => {
    setErr(null); setMsg(null);
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "Clinical source text", extensions: ["txt"] }],
    });
    if (typeof selected !== "string") return;
    try {
      const n = await coordinatorIpc.importDocumentFile(patient.id, selected);
      setMsg(`Imported ${n} document section${n === 1 ? "" : "s"}.`);
      onChanged();
    } catch (e) {
      setErr(String(e).replace(/^.*Error: /, ""));
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

      <div className="field-label">Source documents</div>
      <div style={{ fontSize: 11, color: "var(--muted-2)", marginBottom: 6 }}>
        Upload one combined <code>.txt</code>. Sections are split by headers
        “Txt Imaging”, “Txt Clinical Notes”, “Txt Pathology”, “Txt Labs”.
        {patient.documentCount > 0 && ` Currently ${patient.documentCount} section(s) stored (re-upload replaces).`}
      </div>
      <button className="btn btn-ghost btn-sm" style={{ marginBottom: 12 }} onClick={uploadCombined}>
        📄 Upload combined source file…
      </button>

      <div className="field-label">Import AI output (JSON)</div>
      <textarea className="text-input" style={{ minHeight: 80, resize: "vertical", fontFamily: "var(--font-mono)", fontSize: 11 }} placeholder='{"session_metadata": {...}, "phase3_recommendations": [...]}' value={llm} onChange={(e) => setLlm(e.currentTarget.value)} />
      <button className="btn btn-primary btn-sm" style={{ marginTop: 8 }} disabled={!llm.trim()} onClick={importLlm}>Validate & import</button>

      {msg && <div style={{ color: "var(--success)", fontSize: 12, marginTop: 8 }}>{msg}</div>}
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

function ResponsesTab() {
  const [view, setView] = useState<ResponsesView | null>(null);
  const load = async () => {
    try {
      setView(await coordinatorIpc.responses());
    } catch (e) {
      console.error(e);
    }
  };
  useEffect(() => {
    load();
  }, []);

  return (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 1.5fr", gap: 24, alignItems: "start" }}>
      <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
        <ImportResponsePanel onImported={load} resultsCount={view?.responseCount ?? 0} />
        <ExportAnalysisPanel />
        <BatchesPanel
          batches={view?.batches ?? []}
          onDeleteResponse={async (aid, rid) => {
            await coordinatorIpc.deleteResponse(aid, rid);
            load();
          }}
        />
      </div>
      <AggregatePanel view={view} />
    </div>
  );
}

function ExportAnalysisPanel() {
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const exportAnalysis = async () => {
    setMsg(null); setErr(null);
    const stamp = new Date().toISOString().slice(0, 10);
    const destination = await saveDialog({
      defaultPath: `TumorBoard_Analysis_${stamp}.json`,
      filters: [{ name: "Analysis JSON", extensions: ["json"] }],
    });
    if (!destination) return;
    try {
      const r = await coordinatorIpc.exportAnalysis(destination);
      setMsg(`Exported ${r.recordCount} records. JSON + CSV written next to each other.`);
    } catch (e) {
      setErr("Could not export analysis.");
      console.error(e);
    }
  };

  return (
    <div style={{ background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 14 }}>
      <SectionTitle>Analysis export</SectionTitle>
      <p style={{ fontSize: 12, color: "var(--muted)", margin: "0 0 10px" }}>
        Pooled, analysis-ready export across all imported responses — a lossless JSON and a flat CSV
        (one row per reviewer × patient × recommendation) with timing, note text, original AI text,
        accept/dismiss/alter flags, edit distance/similarity, and authorship breakdown.
      </p>
      <button className="btn btn-primary btn-sm" onClick={exportAnalysis}>Export analysis (JSON + CSV)…</button>
      {msg && <div style={{ color: "var(--success)", fontSize: 12, marginTop: 8 }}>{msg}</div>}
      {err && <div className="form-error">{err}</div>}
    </div>
  );
}

function ReviewersTab({ allPatients }: { allPatients: CoordPatient[] }) {
  const [grid, setGrid] = useState<ReviewerGrid | null>(null);
  const load = () => coordinatorIpc.reviewers().then(setGrid).catch((e) => console.error(e));
  useEffect(() => {
    load();
  }, []);

  if (!grid) return <div className="doc-empty">Loading…</div>;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16, maxWidth: 940 }}>
      <AddReviewerForm onAdded={load} />
      {grid.reviewers.length === 0 && (
        <div className="doc-empty">No reviewers yet. Add a reviewer above, assign patients, then generate a package.</div>
      )}
      {grid.reviewers.map((rv) => (
        <ReviewerRow key={rv.reviewerId} reviewer={rv} allPatients={allPatients} onChanged={load} />
      ))}
    </div>
  );
}

function AddReviewerForm({ onAdded }: { onAdded: () => void }) {
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const add = async () => {
    if (!id.trim()) return;
    await coordinatorIpc.addReviewer(id.trim(), name.trim());
    setId(""); setName("");
    onAdded();
  };
  return (
    <div style={{ background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 14, display: "flex", gap: 8, alignItems: "center" }}>
      <input className="text-input" style={{ maxWidth: 180 }} placeholder="Reviewer ID" value={id} onChange={(e) => setId(e.currentTarget.value)} />
      <input className="text-input" style={{ maxWidth: 220 }} placeholder="Reviewer name (optional)" value={name} onChange={(e) => setName(e.currentTarget.value)} />
      <button className="btn btn-primary btn-sm" disabled={!id.trim()} onClick={add}>+ Add reviewer</button>
    </div>
  );
}

function ReviewerRow({
  reviewer,
  allPatients,
  onChanged,
}: {
  reviewer: import("../services/ipc").GridReviewer;
  allPatients: CoordPatient[];
  onChanged: () => void;
}) {
  const [pw, setPw] = useState("");
  const [showGen, setShowGen] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const assigned = new Set(reviewer.assignedPatients.map((p) => p.patientId));

  const toggle = async (pid: string) => {
    const next = new Set(assigned);
    next.has(pid) ? next.delete(pid) : next.add(pid);
    await coordinatorIpc.assignPatients(reviewer.reviewerId, [...next]);
    onChanged();
  };

  const generate = async () => {
    if (!pw || assigned.size === 0) return;
    setErr(null); setMsg(null);
    const destination = await saveDialog({
      defaultPath: `TumorBoard_${reviewer.reviewerId}.atb`,
      filters: [{ name: "AI Tumor Board Assignment", extensions: ["atb"] }],
    });
    if (!destination) return;
    try {
      const r = await coordinatorIpc.buildPackage(
        reviewer.reviewerId,
        reviewer.displayName ?? reviewer.reviewerId,
        [...assigned],
        pw,
        destination
      );
      setMsg(`Package built: ${r.assignmentId} (${r.patientCount} patients).`);
      setPw(""); setShowGen(false);
      onChanged();
    } catch (e) {
      setErr(String(e).replace(/^.*failed validation: /, "Validation failed: "));
    }
  };

  return (
    <div style={{ background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 16 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 10 }}>
        <div>
          <strong style={{ fontSize: 15, color: "#1e293b" }}>{reviewer.displayName ?? reviewer.reviewerId}</strong>
          <span style={{ fontSize: 12, color: "var(--muted)", marginLeft: 8 }}>{reviewer.reviewerId}</span>
        </div>
        <button
          className="btn btn-ghost btn-sm"
          onClick={() => {
            if (window.confirm(`Delete reviewer ${reviewer.reviewerId} and all their batches/responses? This cannot be undone.`)) {
              coordinatorIpc.deleteReviewer(reviewer.reviewerId).then(onChanged);
            }
          }}
        >
          Delete reviewer
        </button>
      </div>

      {/* Assign patients */}
      <div className="field-label">Assign patients / models ({assigned.size})</div>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 6, marginBottom: 10 }}>
        {allPatients.map((p) => {
          const on = assigned.has(p.id);
          const notReady = p.documentCount === 0 || p.recommendationCount === 0;
          return (
            <button
              key={p.id}
              className={`btn btn-sm ${on ? "btn-primary" : "btn-ghost"}`}
              title={notReady ? "Missing a document or AI output" : ""}
              onClick={() => toggle(p.id)}
            >
              {p.researchId ?? p.id} · {p.cancerType ?? p.modelId ?? "—"}
              {notReady && <span style={{ color: on ? "#fde68a" : "var(--warning)", marginLeft: 4 }}>⚠</span>}
            </button>
          );
        })}
        {allPatients.length === 0 && <span style={{ fontSize: 12, color: "var(--muted-2)" }}>No patients in the study yet.</span>}
      </div>

      {/* Generate package */}
      {showGen ? (
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input className="text-input" type="password" style={{ maxWidth: 220 }} placeholder="Assignment password" value={pw} onChange={(e) => setPw(e.currentTarget.value)} />
          <button className="btn btn-success btn-sm" disabled={!pw || assigned.size === 0} onClick={generate}>Build &amp; save .atb</button>
          <button className="btn btn-ghost btn-sm" onClick={() => { setShowGen(false); setPw(""); }}>Cancel</button>
        </div>
      ) : (
        <button className="btn btn-success btn-sm" disabled={assigned.size === 0} onClick={() => setShowGen(true)}>
          Generate package for this reviewer…
        </button>
      )}
      {msg && <div style={{ color: "var(--success)", fontSize: 12, marginTop: 8 }}>{msg}</div>}
      {err && <div className="form-error">{err}</div>}

      {/* Batches already sent */}
      {reviewer.assignments.length > 0 && (
        <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 6 }}>
          <div className="field-label">Batches sent ({reviewer.assignments.length})</div>
          {reviewer.assignments.map((a) => (
            <div key={a.assignmentId} style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 11.5 }}>
              <span style={{ fontFamily: "var(--font-mono)", color: "var(--muted)" }}>{a.assignmentId.slice(0, 16)}…</span>
              <span style={{ color: "var(--muted-2)" }}>{a.createdAt.slice(0, 10)}</span>
              <span style={{ color: "var(--text-2)" }}>{a.patients.map((p) => p.researchId ?? p.patientId).join(", ")}</span>
              <span className={`status-chip ${a.responded ? "complete" : "not_started"}`}>{a.responded ? "Responded" : "Awaiting"}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function BatchesPanel({ batches, onDeleteResponse }: { batches: Batch[]; onDeleteResponse: (assignmentId: string, reviewerId: string) => void }) {
  return (
    <div style={{ background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 14 }}>
      <SectionTitle>Sent batches ({batches.length})</SectionTitle>
      {batches.length === 0 ? (
        <div style={{ fontSize: 12, color: "var(--muted-2)" }}>No assignment packages have been built yet.</div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {batches.map((b) => (
            <div key={b.assignmentId} style={{ display: "flex", alignItems: "center", gap: 10, borderBottom: "1px solid var(--border-2)", paddingBottom: 8 }}>
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 13, fontWeight: 600, color: "#1e293b" }}>
                  {b.displayName ?? b.reviewerId}
                </div>
                <div style={{ fontSize: 11, color: "var(--muted)" }}>
                  {b.reviewerId} · {b.patientCount} patient{b.patientCount === 1 ? "" : "s"} · {b.createdAt.slice(0, 10)}
                </div>
              </div>
              <span className={`status-chip ${b.responded ? "complete" : "not_started"}`}>
                {b.responded ? "Responded" : "Awaiting"}
              </span>
              {b.responded && (
                <button
                  className="nb-remove-btn"
                  title="Delete this uploaded response"
                  onClick={() => {
                    if (window.confirm(`Delete the uploaded response from ${b.reviewerId}? The aggregate will update. This cannot be undone.`)) {
                      onDeleteResponse(b.assignmentId, b.reviewerId);
                    }
                  }}
                >
                  ✕
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function Bar({ accepted, dismissed, ignored }: { accepted: number; dismissed: number; ignored: number }) {
  const total = Math.max(1, accepted + dismissed + ignored);
  const seg = (n: number, color: string) =>
    n > 0 ? <div style={{ width: `${(n / total) * 100}%`, background: color }} title={`${n}`} /> : null;
  return (
    <div style={{ display: "flex", height: 8, borderRadius: 4, overflow: "hidden", background: "var(--border-2)" }}>
      {seg(accepted, "#16a34a")}
      {seg(dismissed, "#dc2626")}
      {seg(ignored, "#cbd5e1")}
    </div>
  );
}

function AggregatePanel({ view }: { view: ResponsesView | null }) {
  if (!view) return <div className="doc-empty">Loading…</div>;
  return (
    <div style={{ background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 16 }}>
      <SectionTitle>Aggregated findings</SectionTitle>
      <div style={{ fontSize: 13, color: "var(--text-2)", marginBottom: 6 }}>
        <strong>{view.responseCount}</strong> response{view.responseCount === 1 ? "" : "s"} from{" "}
        <strong>{view.reviewers.length}</strong> physician{view.reviewers.length === 1 ? "" : "s"}
        {view.reviewers.length > 0 && (
          <span style={{ color: "var(--muted)" }}> ({view.reviewers.join(", ")})</span>
        )}
        .
      </div>
      <div style={{ fontSize: 10.5, color: "var(--muted-2)", marginBottom: 14 }}>
        <span style={{ color: "#16a34a" }}>■ accepted</span>{"  "}
        <span style={{ color: "#dc2626" }}>■ dismissed</span>{"  "}
        <span style={{ color: "#94a3b8" }}>■ not used</span>
      </div>

      {view.patients.length === 0 ? (
        <div className="doc-empty">No responses imported yet. Import a physician’s .atbr to aggregate.</div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
          {view.patients.map((p: AggPatient) => (
            <div key={p.patientId}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 6 }}>
                <strong style={{ fontSize: 13, color: "#1e293b" }}>{p.researchId ?? p.patientId}</strong>
                <span style={{ fontSize: 11, color: "var(--muted)" }}>
                  {p.responseCount} response{p.responseCount === 1 ? "" : "s"} · avg {p.avgPctPhysicianAuthored}% physician-authored
                </span>
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                {p.recommendations.map((r) => (
                  <div key={r.recommendationId}>
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: 11.5, color: "var(--text-2)", marginBottom: 3 }}>
                      <span>{r.title}</span>
                      <span style={{ color: "var(--muted)" }}>
                        {r.accepted}✓ · {r.dismissed}✕ · {r.ignored}○
                      </span>
                    </div>
                    <Bar accepted={r.accepted} dismissed={r.dismissed} ignored={r.ignored} />
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
