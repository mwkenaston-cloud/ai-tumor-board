import { useState } from "react";
import { useApp } from "../app/AppContext";
import type { PatientSummary } from "../models/types";

export default function PhysicianLobby() {
  const { assignment, actions } = useApp();
  const [confirming, setConfirming] = useState<PatientSummary | null>(null);
  if (!assignment) return null;

  const done = assignment.patients.filter((p) => p.status === "complete").length;
  const allComplete = done === assignment.patients.length && assignment.patients.length > 0;

  return (
    <div className="lobby">
      <div className="lobby-inner">
        <div className="lobby-guide">
          <h2>{assignment.studyTitle}</h2>
          <p className="instructions">{assignment.instructions}</p>
          <p style={{ marginTop: 16, fontSize: 13, color: "var(--muted)" }}>
            Assigned reviewer: <strong>{assignment.reviewerDisplayName}</strong>
            {assignment.contactEmail && <> · Questions: {assignment.contactEmail}</>}
          </p>

          {assignment.state !== "submitted" && (
            <button
              className="btn btn-ghost btn-sm"
              style={{ marginTop: 20 }}
              onClick={() => {
                if (
                  window.confirm(
                    "Reset this session? This permanently discards ALL of your notes, decisions, surveys, and timers for every patient and starts over. This cannot be undone."
                  )
                ) {
                  actions.resetSession();
                }
              }}
            >
              ↺ Reset session
            </button>
          )}
        </div>

        <div className="lobby-queue">
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 12 }}>
            <h3 style={{ margin: 0, fontSize: 14, color: "#1e293b" }}>Assigned patients</h3>
            <span style={{ fontSize: 12, color: "var(--muted)" }}>
              {done}/{assignment.patients.length} complete
            </span>
          </div>
          <div className="queue-list">
            {assignment.patients.map((p) => (
              <div key={p.id} className="queue-row" onClick={() => setConfirming(p)}>
                <span className="q-id">{p.researchId ?? p.id}</span>
                <span className="q-label">{p.displayLabel}</span>
                <span className={`status-chip ${p.status}`}>{p.status.replace("_", " ")}</span>
              </div>
            ))}
          </div>

          <button
            className="btn btn-success"
            style={{ marginTop: 16, width: "100%", justifyContent: "center" }}
            disabled={!allComplete}
            onClick={actions.startCompletion}
            title={allComplete ? "" : "Complete all assigned patients first"}
          >
            {allComplete ? "Finish & submit assignment" : `Complete all patients to submit (${done}/${assignment.patients.length})`}
          </button>
        </div>
      </div>

      {confirming && (
        <div
          style={{ position: "fixed", inset: 0, background: "rgba(15,23,42,0.55)", display: "flex", alignItems: "center", justifyContent: "center", zIndex: 500 }}
          onClick={() => setConfirming(null)}
        >
          <div className="card" style={{ maxWidth: 420 }} onClick={(e) => e.stopPropagation()}>
            <h2 style={{ fontSize: 19 }}>Begin review?</h2>
            <p>
              You are about to begin reviewing <strong>{confirming.researchId ?? confirming.id}</strong>
              {confirming.displayLabel ? ` — ${confirming.displayLabel}` : ""}. The review timer will
              start when you begin.
            </p>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", marginTop: 16 }}>
              <button className="btn btn-ghost" onClick={() => setConfirming(null)}>Cancel</button>
              <button
                className="btn btn-success"
                onClick={() => {
                  const id = confirming.id;
                  setConfirming(null);
                  actions.openPatient(id);
                }}
              >
                Begin review
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
