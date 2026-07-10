import { useApp } from "../app/AppContext";
import { attributionMetrics } from "../services/noteBlocks";

export default function SubmissionScreen() {
  const { assignment, patients } = useApp();
  if (!assignment) return null;

  const list = Object.values(patients);
  const totalRecs = list.reduce((n, p) => n + p.recommendations.length, 0);
  const used = list.reduce(
    (n, p) => n + p.decisions.filter((d) => d.status.startsWith("used")).length,
    0
  );
  const dismissed = list.reduce(
    (n, p) => n + p.decisions.filter((d) => d.status === "dismissed").length,
    0
  );

  return (
    <div className="center-screen">
      <div className="card" style={{ maxWidth: 560, textAlign: "center" }}>
        <div style={{ fontSize: 40 }}>✅</div>
        <h2>Assignment submitted</h2>
        <p>
          Your responses for <strong>{assignment.studyTitle}</strong> are complete. In the packaged
          app this creates an encrypted <code>.atbr</code> response file to return to the study
          coordinator. This assignment is now read-only.
        </p>
        <div
          style={{
            textAlign: "left",
            background: "var(--surface-2)",
            border: "1px solid var(--border-2)",
            borderRadius: 10,
            padding: 16,
            marginTop: 8,
          }}
        >
          <div className="field-label">Summary</div>
          <ul style={{ margin: "8px 0 0", paddingLeft: 18, fontSize: 13, color: "var(--text-2)", lineHeight: 1.9 }}>
            <li>Patients reviewed: {list.filter((p) => p.status === "complete").length}/{list.length}</li>
            <li>AI recommendations: {totalRecs} total · {used} used · {dismissed} dismissed</li>
            {list.map((p) => {
              const m = attributionMetrics(p.noteBlocks);
              return (
                <li key={p.id}>
                  {p.researchId ?? p.id}: {m.wordCount} words · {m.pctPhysicianOriginal}% physician-authored
                </li>
              );
            })}
          </ul>
        </div>
      </div>
    </div>
  );
}
