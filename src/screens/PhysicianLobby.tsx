import { useApp } from "../app/AppContext";

export default function PhysicianLobby() {
  const { assignment, actions } = useApp();
  if (!assignment) return null;

  const done = assignment.patients.filter((p) => p.status === "complete").length;

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
              <div key={p.id} className="queue-row" onClick={() => actions.openPatient(p.id)}>
                <span className="q-id">{p.researchId ?? p.id}</span>
                <span className="q-label">{p.displayLabel}</span>
                <span className={`status-chip ${p.status}`}>{p.status.replace("_", " ")}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
