import { useState } from "react";
import type { Patient, TimelineEvent } from "../models/types";
import SourceDocumentViewer from "./SourceDocumentViewer";

function isObj(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}
function str(v: unknown): string | null {
  return typeof v === "string" && v.trim().length > 0 ? v : null;
}
function num(v: unknown): number | null {
  return typeof v === "number" ? v : null;
}
function arr(v: unknown): Record<string, unknown>[] {
  return Array.isArray(v) ? (v as Record<string, unknown>[]) : [];
}

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
    <div style={{ marginBottom: 20 }}>
      <h3 style={{ fontSize: 12, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--muted)", margin: "0 0 8px" }}>{title}</h3>
      {children}
    </div>
  );
}

/** A prominent, numbered top-level grouping so the reader can see how a block of
 *  information is organized and why it flows from top to bottom. */
function GroupHeader({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <div style={{ margin: "0 0 12px", paddingBottom: 6, borderBottom: "2px solid var(--border-2)" }}>
      <h2 style={{ fontSize: 15, fontWeight: 700, color: "#1e293b", margin: 0 }}>{title}</h2>
      {subtitle && <div style={{ fontSize: 11.5, color: "var(--muted)", marginTop: 2, lineHeight: 1.45 }}>{subtitle}</div>}
    </div>
  );
}

function KeyValueList({ items, fields }: { items: Record<string, unknown>[]; fields: string[] }) {
  if (!items || items.length === 0) return <div className="doc-empty">None documented.</div>;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {items.map((it, i) => (
        <div key={i} style={{ background: "var(--surface)", border: "1px solid var(--border-2)", borderRadius: 8, padding: "8px 10px" }}>
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

// ── Comorbidity rendering (v1.2 rich object, falls back to v1.0 array) ────────
function ComorbidityView({ comorbidities }: { comorbidities: unknown }) {
  if (Array.isArray(comorbidities)) {
    return <KeyValueList items={arr(comorbidities)} fields={["condition", "status", "details", "source_quote"]} />;
  }
  if (!isObj(comorbidities)) return <div className="doc-empty">None documented.</div>;

  const summary = isObj(comorbidities.comorbidity_summary) ? comorbidities.comorbidity_summary : {};
  const cci = isObj(summary.cci_score_overview) ? summary.cci_score_overview : {};
  const drivers = arr(summary.primary_score_drivers);
  const activeFlags = arr(summary.active_treatment_relevant_flags);
  const gaps = isObj(summary.confidence_and_data_gaps) ? summary.confidence_and_data_gaps : {};
  const narrative = str(summary.overall_burden_narrative);
  const flags = arr(comorbidities.treatment_relevant_flags);
  const other = arr(comorbidities.other_comorbidities);
  const presentFlags = flags.filter((f) => str(f.status) === "present");
  const negativeFlags = flags.filter((f) => str(f.status) !== "present").map((f) => str(f.category)).filter(Boolean);

  const cciUnadj = num(cci.unadjusted_score);
  const cciAge = num(cci.age_adjusted_score);
  const surv = num(cci.estimated_10yr_survival_pct);

  // Ordered top → bottom so the reader moves from the big picture to the detail
  // and finally to the caveats: overall summary → the CCI score → what drives
  // that score → treatment-relevant flags → other conditions → what was ruled
  // out → confidence/data gaps.
  return (
    <div>
      {narrative && (
        <Section title="Overall comorbidity burden">
          <div style={{ background: "#eff6ff", border: "1px solid #bfdbfe", borderRadius: 10, padding: "10px 12px", fontSize: 12.5, color: "#1e293b", lineHeight: 1.6 }}>
            {narrative}
          </div>
        </Section>
      )}

      <Section title="Charlson Comorbidity Index (CCI)">
        {cciUnadj != null || cciAge != null || surv != null ? (
          <div style={{ display: "flex", gap: 10, flexWrap: "wrap", marginBottom: 6 }}>
            {cciUnadj != null && <ScorePill label="CCI" value={String(cciUnadj)} />}
            {cciAge != null && <ScorePill label="Age-adjusted" value={String(cciAge)} />}
            {surv != null && <ScorePill label="Est. 10-yr survival" value={`${surv}%`} />}
          </div>
        ) : (
          <div style={{ fontSize: 12, color: "var(--muted-2)" }}>Score not calculated.</div>
        )}
        {str(cci.interpretation) && (
          <div style={{ fontSize: 12, color: "var(--text-2)", lineHeight: 1.55 }}>{str(cci.interpretation)}</div>
        )}
      </Section>

      {drivers.length > 0 && (
        <Section title="What drives that score">
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            {drivers.map((d, i) => (
              <div key={i} style={{ fontSize: 12, color: "var(--text-2)", lineHeight: 1.5 }}>
                <strong>{str(d.condition)}</strong>
                {num(d.cci_weight) != null && <span style={{ color: "var(--muted)" }}> (weight {num(d.cci_weight)})</span>}
                {str(d.weighting_rationale) && <> — {str(d.weighting_rationale)}</>}
              </div>
            ))}
          </div>
        </Section>
      )}

      {(presentFlags.length > 0 || activeFlags.length > 0) && (
        <Section title="Treatment-relevant flags (active)">
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {(activeFlags.length > 0 ? activeFlags : presentFlags).map((f, i) => (
              <div key={i} style={{ background: "#fffbeb", border: "1px solid #fde68a", borderRadius: 8, padding: "8px 10px" }}>
                <div style={{ fontSize: 12.5, fontWeight: 700, color: "#92400e" }}>
                  {str(f.category)}
                  {str(f.clinical_detail) && <span style={{ fontWeight: 500 }}> — {str(f.clinical_detail)}</span>}
                  {str(f.severity_or_stage) && <span style={{ fontWeight: 500 }}> — {str(f.severity_or_stage)}</span>}
                </div>
                {str(f.treatment_implication) && (
                  <div style={{ fontSize: 12, color: "var(--text-2)", lineHeight: 1.5, marginTop: 3 }}>{str(f.treatment_implication)}</div>
                )}
              </div>
            ))}
          </div>
        </Section>
      )}

      {other.length > 0 && (
        <Section title="Other documented comorbidities">
          <KeyValueList items={other} fields={["condition", "status", "details", "source_quote"]} />
        </Section>
      )}

      {negativeFlags.length > 0 && (
        <Section title="Screened & negative / undocumented">
          <div style={{ fontSize: 12, color: "var(--muted)", lineHeight: 1.5 }}>{negativeFlags.join(", ")}.</div>
        </Section>
      )}

      {(str(gaps.suspected_but_undocumented) || str(gaps.missing_data_points)) && (
        <Section title="Confidence & data gaps">
          {str(gaps.suspected_but_undocumented) && (
            <div style={{ fontSize: 12, color: "var(--text-2)", lineHeight: 1.5, marginBottom: 4 }}>
              <span style={{ color: "var(--muted)" }}>Suspected/undocumented:</span> {str(gaps.suspected_but_undocumented)}
            </div>
          )}
          {str(gaps.missing_data_points) && (
            <div style={{ fontSize: 12, color: "var(--text-2)", lineHeight: 1.5 }}>
              <span style={{ color: "var(--muted)" }}>Missing data:</span> {str(gaps.missing_data_points)}
            </div>
          )}
        </Section>
      )}
    </div>
  );
}

function ScorePill({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ background: "var(--surface-2)", border: "1px solid var(--border-2)", borderRadius: 8, padding: "6px 12px" }}>
      <div style={{ fontSize: 10, color: "var(--muted)", textTransform: "uppercase", letterSpacing: 0.4 }}>{label}</div>
      <div style={{ fontSize: 16, fontWeight: 700, color: "#1e293b" }}>{value}</div>
    </div>
  );
}

function Genetics({ family }: { family: Record<string, unknown> }) {
  const famCancer = arr(family.family_history_of_cancer);
  const germline = isObj(family.germline_testing) ? family.germline_testing : null;
  const somatic = isObj(family.somatic_testing) ? family.somatic_testing : null;
  const biomarkers = somatic && isObj(somatic.biomarkers) ? somatic.biomarkers : null;

  return (
    <>
      <Section title="Family history of cancer">
        <KeyValueList items={famCancer} fields={["relative", "cancer_type", "age_at_diagnosis", "source_quote"]} />
      </Section>
      {germline && (
        <Section title="Germline testing">
          <div style={{ fontSize: 12, color: "var(--text-2)", lineHeight: 1.6 }}>
            {str(germline.test_name) && <div><span style={{ color: "var(--muted)" }}>Test:</span> {str(germline.test_name)}</div>}
            <div><span style={{ color: "var(--muted)" }}>Result:</span> {str(germline.result) ?? (germline.performed ? "Performed" : "Not performed")}</div>
            {arr(germline.pathogenic_variants).length > 0 && (
              <div><span style={{ color: "var(--muted)" }}>Pathogenic:</span> {(germline.pathogenic_variants as unknown[]).map(String).join(", ")}</div>
            )}
          </div>
        </Section>
      )}
      {somatic && (
        <Section title="Somatic tumor profiling">
          <div style={{ fontSize: 12, color: "var(--text-2)", lineHeight: 1.6 }}>
            {str(somatic.assay) && <div><span style={{ color: "var(--muted)" }}>Assay:</span> {str(somatic.assay)}</div>}
            {arr(somatic.alterations).length > 0 && (
              <div><span style={{ color: "var(--muted)" }}>Alterations:</span> {(somatic.alterations as unknown[]).map(String).join(", ")}</div>
            )}
            {biomarkers && (
              <div>
                {Object.entries(biomarkers).filter(([, v]) => str(v)).map(([k, v]) => (
                  <span key={k} style={{ marginRight: 10 }}><span style={{ color: "var(--muted)" }}>{k}:</span> {String(v)}</span>
                ))}
              </div>
            )}
          </div>
        </Section>
      )}
    </>
  );
}

export default function PatientContextView({ patient }: { patient: Patient }) {
  const [view, setView] = useState<"timeline" | "history">("timeline");
  const ctx = patient.context ?? {};
  const timeline = ctx.timeline ?? [];
  const family = (ctx.family_history ?? {}) as Record<string, unknown>;

  return (
    <div className="review">
      {/* Left half: profile + timeline/history toggle */}
      <div style={{ flex: 1, minWidth: 0, overflowY: "auto", padding: "16px 20px", background: "var(--surface-2)", borderRight: "1px solid var(--border-2)" }}>
        {ctx.patient_profile && (
          <div style={{ marginBottom: 16 }}>
            <GroupHeader title="Patient summary" />
            <div style={{ fontSize: 13, color: "var(--text-2)", lineHeight: 1.6 }}>{ctx.patient_profile}</div>
          </div>
        )}
        <div style={{ display: "flex", gap: 6, marginBottom: 14 }}>
          <button className={`btn btn-sm ${view === "timeline" ? "btn-primary" : "btn-ghost"}`} onClick={() => setView("timeline")}>
            Clinical timeline
          </button>
          <button className={`btn btn-sm ${view === "history" ? "btn-primary" : "btn-ghost"}`} onClick={() => setView("history")}>
            Relevant history
          </button>
        </div>

        {view === "timeline" ? (
          <Timeline events={timeline} />
        ) : (
          <>
            <div style={{ marginBottom: 22 }}>
              <GroupHeader
                title="Comorbidity burden"
                subtitle="From the overall picture down to the specifics: summary, the CCI score, what drives it, treatment-relevant flags, and finally what's uncertain."
              />
              <ComorbidityView comorbidities={ctx.comorbidities} />
            </div>
            <div>
              <GroupHeader title="Genetics & family history" />
              <Genetics family={family} />
            </div>
          </>
        )}
      </div>

      {/* Right half: raw source documents (unchanged space) */}
      <SourceDocumentViewer patient={patient} fill />
    </div>
  );
}
