import { useState } from "react";
import { useApp } from "../app/AppContext";
import type { DecisionStatus } from "../models/types";

type RecDisposition = "accepted" | "dismissed" | "ignored";

const REASONS: Record<RecDisposition, string[]> = {
  accepted: [
    "Clinically appropriate",
    "Consistent with my own plan",
    "Strong supporting evidence",
    "Raised a consideration I had not made",
    "Other",
  ],
  dismissed: [
    "Not clinically appropriate",
    "Contraindicated for this patient",
    "Insufficient evidence",
    "Patient preference / values",
    "Other",
  ],
  ignored: [
    "Not relevant to the question",
    "Redundant with another recommendation",
    "Already covered in my plan",
    "Uncertain about it",
    "Other",
  ],
};

const DISPOSITION_LABEL: Record<RecDisposition, string> = {
  accepted: "Accepted (inserted)",
  dismissed: "Dismissed",
  ignored: "Not used",
};

const DISPOSITION_PROMPT: Record<RecDisposition, string> = {
  accepted: "Why did you accept this recommendation?",
  dismissed: "Why did you dismiss this recommendation?",
  ignored: "Why did you not use this recommendation?",
};

function dispositionOf(status: DecisionStatus | undefined): RecDisposition {
  if (status === "used" || status === "used-and-edited") return "accepted";
  if (status === "dismissed") return "dismissed";
  return "ignored";
}

export default function PatientSurvey() {
  const { currentPatient, actions } = useApp();
  const [answers, setAnswers] = useState<Record<string, string>>({});
  if (!currentPatient) return null;
  const p = currentPatient;

  const set = (key: string, value: string) => setAnswers((a) => ({ ...a, [key]: value }));

  const recRows = p.recommendations.map((rec) => {
    const status = p.decisions.find((d) => d.recommendationId === rec.id)?.status;
    return { rec, disposition: dispositionOf(status) };
  });

  // Require the overall rating and a reason for each accepted/dismissed rec.
  const overallDone = !!answers["overall:ai_quality"];
  const reasonsDone = recRows.every(
    ({ rec, disposition }) =>
      disposition === "ignored" || !!answers[`rec:${rec.id}:reason`]
  );
  const complete = overallDone && reasonsDone;

  return (
    <div className="center-screen" style={{ alignItems: "flex-start" }}>
      <div className="card" style={{ maxWidth: 620, margin: "24px auto" }}>
        <span className="field-label">Per-patient survey</span>
        <h2 style={{ fontSize: 19 }}>{p.cancerType ?? p.displayLabel}</h2>
        <p style={{ marginBottom: 18 }}>
          A few questions about your review of this patient before returning to the queue.
        </p>

        <div style={{ marginBottom: 22 }}>
          <label className="field-label" style={{ textTransform: "none", fontSize: 13, color: "#1e293b" }}>
            Overall, how clinically appropriate were the AI recommendations for this patient?
          </label>
          <div style={{ display: "flex", gap: 6 }}>
            {[1, 2, 3, 4, 5].map((n) => (
              <button
                key={n}
                className={`btn btn-sm ${answers["overall:ai_quality"] === String(n) ? "btn-primary" : "btn-ghost"}`}
                style={{ width: 40, justifyContent: "center" }}
                onClick={() => set("overall:ai_quality", String(n))}
              >
                {n}
              </button>
            ))}
          </div>
          <div style={{ display: "flex", justifyContent: "space-between", fontSize: 10.5, color: "var(--muted-2)", marginTop: 4, maxWidth: 236 }}>
            <span>Poor</span>
            <span>Excellent</span>
          </div>
        </div>

        <div className="field-label" style={{ marginBottom: 8 }}>Per-recommendation feedback</div>
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          {recRows.map(({ rec, disposition }) => (
            <div key={rec.id} style={{ border: "1px solid var(--border-2)", borderRadius: 8, padding: 12 }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 6 }}>
                <strong style={{ fontSize: 13, color: "#1e293b" }}>{rec.title ?? rec.id}</strong>
                <span className={`status-chip ${disposition === "accepted" ? "complete" : disposition === "dismissed" ? "reopened" : "not_started"}`}>
                  {DISPOSITION_LABEL[disposition]}
                </span>
              </div>
              <div style={{ fontSize: 12, color: "var(--muted)", marginBottom: 8 }}>{DISPOSITION_PROMPT[disposition]}</div>
              <select
                className="text-input"
                value={answers[`rec:${rec.id}:reason`] ?? ""}
                onChange={(e) => set(`rec:${rec.id}:reason`, e.currentTarget.value)}
              >
                <option value="">Select a reason…</option>
                {REASONS[disposition].map((r) => (
                  <option key={r} value={r}>{r}</option>
                ))}
              </select>
              <textarea
                className="text-input"
                style={{ marginTop: 8, minHeight: 44, resize: "vertical", fontSize: 12.5 }}
                placeholder="Additional notes (optional)"
                value={answers[`rec:${rec.id}:notes`] ?? ""}
                onChange={(e) => set(`rec:${rec.id}:notes`, e.currentTarget.value)}
              />
            </div>
          ))}
          {recRows.length === 0 && (
            <div style={{ fontSize: 12, color: "var(--muted-2)" }}>No recommendations to review for this patient.</div>
          )}
        </div>

        <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 22 }}>
          <button
            className="btn btn-primary"
            disabled={!complete}
            onClick={() => actions.submitPatientSurvey(p.id, answers)}
          >
            Save &amp; return to queue
          </button>
        </div>
      </div>
    </div>
  );
}
