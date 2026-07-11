import type { Patient, TimelineEvent } from "../models/types";
import SourceDocumentViewer from "./SourceDocumentViewer";

function Timeline({ events }: { events: TimelineEvent[] }) {
  if (events.length === 0) return <div className="doc-empty">No timeline available.</div>;
  return (
    <div style={{ position: "relative", paddingLeft: 16 }}>
      {events.map((e, i) => (
        <div key={i} style={{ position: "relative", paddingBottom: 14 }}>
          <div style={{ position: "absolute", left: -16, top: 4, width: 8, height: 8, borderRadius: "50%", background: "var(--primary-bright)" }} />
          {i < events.length - 1 && (
            <div style={{ position: "absolute", left: -12.5, top: 12, bottom: 0, width: 1, background: "var(--border-2)" }} />
          )}
          <div style={{ fontSize: 11, fontWeight: 700, color: "var(--primary)", fontFamily: "var(--font-mono)" }}>
            {e.date ?? "—"} {e.event_type && <span style={{ color: "var(--muted)", fontWeight: 600 }}>· {e.event_type}</span>}
          </div>
          <div style={{ fontSize: 12.5, color: "var(--text-2)", lineHeight: 1.5, marginTop: 2 }}>{e.finding}</div>
          {e.source_quote && (
            <div style={{ fontSize: 11, color: "var(--muted)", fontStyle: "italic", marginTop: 3, borderLeft: "2px solid var(--border-2)", paddingLeft: 8 }}>
              “{e.source_quote}”
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: 22 }}>
      <h3 style={{ fontSize: 12, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--muted)", margin: "0 0 10px" }}>{title}</h3>
      {children}
    </div>
  );
}

function KeyValueList({ items, fields }: { items: Array<Record<string, unknown>>; fields: string[] }) {
  if (!items || items.length === 0) return <div className="doc-empty">None documented.</div>;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {items.map((it, i) => (
        <div key={i} style={{ background: "var(--surface-2)", border: "1px solid var(--border-2)", borderRadius: 8, padding: "8px 10px" }}>
          {fields.map((f) =>
            it[f] != null && String(it[f]).length > 0 ? (
              <div key={f} style={{ fontSize: 12, color: "var(--text-2)", lineHeight: 1.5 }}>
                <span style={{ color: "var(--muted)", textTransform: "capitalize" }}>{f.replace(/_/g, " ")}:</span>{" "}
                {String(it[f])}
              </div>
            ) : null
          )}
        </div>
      ))}
    </div>
  );
}

export default function PatientContextView({ patient }: { patient: Patient }) {
  const ctx = patient.context ?? {};
  const timeline = ctx.timeline ?? [];
  const comorbidities = (ctx.comorbidities ?? []) as Array<Record<string, unknown>>;
  const family = (ctx.family_history ?? {}) as Record<string, unknown>;
  const famCancer = (family.family_history_of_cancer ?? []) as Array<Record<string, unknown>>;

  return (
    <div className="review">
      <SourceDocumentViewer patient={patient} />
      <div style={{ flex: 1, overflowY: "auto", padding: "16px 20px", background: "var(--surface-2)" }}>
        {ctx.patient_profile && (
          <Section title="Patient profile">
            <div style={{ fontSize: 13, color: "var(--text-2)", lineHeight: 1.6 }}>{ctx.patient_profile}</div>
          </Section>
        )}
        <Section title="Clinical timeline">
          <Timeline events={timeline} />
        </Section>
        <Section title="Comorbidities">
          <KeyValueList items={comorbidities} fields={["condition", "status", "details", "source_quote"]} />
        </Section>
        <Section title="Family history & genetics">
          <KeyValueList items={famCancer} fields={["relative", "cancer_type", "age_at_diagnosis", "source_quote"]} />
        </Section>
      </div>
    </div>
  );
}
