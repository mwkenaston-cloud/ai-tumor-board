import type { Patient } from "../models/types";

/** A section heading with a one-line description of how it aids the decision. */
function Head({ title, desc }: { title: string; desc: string }) {
  return (
    <>
      <h3 style={{ fontSize: 13, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--muted)", marginBottom: 2 }}>{title}</h3>
      <p style={{ fontSize: 11.5, color: "var(--muted)", margin: "0 0 10px", lineHeight: 1.5 }}>{desc}</p>
    </>
  );
}

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
            <Head
              title="Decision points"
              desc="The specific choices this case hinges on. These are the questions your assessment & plan should resolve — a checklist of what to address."
            />
            <ol style={{ margin: 0, paddingLeft: 20 }}>
              {decisionPoints.map((d, i) => (
                <li key={i} style={{ fontSize: 13, color: "var(--text-2)", lineHeight: 1.6, marginBottom: 8 }}>{d}</li>
              ))}
            </ol>
          </section>
        )}

        {factors && (
          <section style={{ marginBottom: 24 }}>
            <Head
              title="Relevant patient factors"
              desc="Patient-specific circumstances — comorbidities, function, preferences, logistics — that should weigh on the choice beyond the tumor itself."
            />
            <div style={{ fontSize: 13, color: "var(--text-2)", lineHeight: 1.65 }}>{factors}</div>
          </section>
        )}

        {Object.keys(perspectives).length > 0 && (
          <section>
            <Head
              title="Specialist perspectives"
              desc="How each discipline sees the case. Comparing their differing views surfaces the trade-offs and helps you pressure-test your own plan."
            />
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
