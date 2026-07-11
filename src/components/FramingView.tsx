import type { Patient } from "../models/types";

export default function FramingView({ patient }: { patient: Patient }) {
  const framing = patient.framing ?? {};
  const decisionPoints = framing.decision_points ?? [];
  const perspectives = framing.specialist_perspectives ?? {};
  const factors = framing.relevant_patient_factors;

  const hasContent =
    decisionPoints.length > 0 || Object.keys(perspectives).length > 0 || !!factors;

  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "20px 28px", background: "var(--surface-2)" }}>
      <div style={{ maxWidth: 900, margin: "0 auto" }}>
        {!hasContent && (
          <div className="doc-empty">
            No decision points or specialist perspectives were provided in the AI output for this
            patient.
          </div>
        )}

        {patient.clinicalQuestion && (
          <div style={{ background: "#eff6ff", border: "1px solid #bfdbfe", borderRadius: 10, padding: "12px 16px", marginBottom: 22 }}>
            <div className="field-label" style={{ color: "#1e40af" }}>Clinical question</div>
            <div style={{ fontSize: 13.5, color: "#1e293b", lineHeight: 1.6 }}>{patient.clinicalQuestion}</div>
          </div>
        )}

        {decisionPoints.length > 0 && (
          <section style={{ marginBottom: 24 }}>
            <h3 style={{ fontSize: 13, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--muted)" }}>Decision points</h3>
            <ol style={{ margin: 0, paddingLeft: 20 }}>
              {decisionPoints.map((d, i) => (
                <li key={i} style={{ fontSize: 13, color: "var(--text-2)", lineHeight: 1.6, marginBottom: 8 }}>{d}</li>
              ))}
            </ol>
          </section>
        )}

        {factors && (
          <section style={{ marginBottom: 24 }}>
            <h3 style={{ fontSize: 13, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--muted)" }}>Relevant patient factors</h3>
            <div style={{ fontSize: 13, color: "var(--text-2)", lineHeight: 1.65 }}>{factors}</div>
          </section>
        )}

        {Object.keys(perspectives).length > 0 && (
          <section>
            <h3 style={{ fontSize: 13, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--muted)" }}>Specialist perspectives</h3>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
              {Object.entries(perspectives).map(([specialty, text]) => (
                <div key={specialty} style={{ background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 14 }}>
                  <div style={{ fontSize: 12, fontWeight: 700, color: "var(--primary)", textTransform: "capitalize", marginBottom: 6 }}>{specialty}</div>
                  <div style={{ fontSize: 12.5, color: "var(--text-2)", lineHeight: 1.55 }}>{text}</div>
                </div>
              ))}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
