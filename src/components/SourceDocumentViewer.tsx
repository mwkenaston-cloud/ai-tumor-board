import { useState } from "react";
import type { Patient } from "../models/types";

const TAB_ORDER = ["notes", "pathology", "imaging", "labs"];
const TAB_LABEL: Record<string, string> = {
  notes: "Notes",
  pathology: "Path",
  imaging: "Imaging",
  labs: "Labs",
};

export default function SourceDocumentViewer({
  patient,
  fill = false,
}: {
  patient: Patient;
  fill?: boolean;
}) {
  const types = Array.from(new Set(patient.documents.map((d) => d.documentType))).sort(
    (a, b) => TAB_ORDER.indexOf(a) - TAB_ORDER.indexOf(b)
  );
  const [active, setActive] = useState(types[0] ?? "notes");
  const doc = patient.documents.find((d) => d.documentType === active);

  // Reader-adjustable text size for the (often dense, small) source text. Same
  // width/wrapping — just larger glyphs, for readability.
  const SIZES = [13, 15, 17, 20];
  const [sizeIdx, setSizeIdx] = useState(0);
  const fontSize = SIZES[sizeIdx];

  return (
    <div className="left-panel" style={fill ? { flex: 1, width: "auto", minWidth: 0 } : undefined}>
      <div className="cq-block">
        <span className="field-label">Clinical question</span>
        <div className="cq-text">{patient.clinicalQuestion ?? "—"}</div>
      </div>

      <div style={{ fontSize: 11.5, color: "var(--muted)", lineHeight: 1.5, padding: "0 2px 8px" }}>
        <strong style={{ color: "var(--text-2)" }}>Source documents.</strong> The full clinical
        material given to the AI — notes, pathology, imaging, and labs. Use them to confirm details
        in the summary and recommendations.
      </div>

      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8 }}>
        <div className="tab-nav" style={{ marginBottom: 0 }}>
          {types.map((t) => (
            <button
              key={t}
              className={`tab-btn ${t === active ? "active" : ""}`}
              onClick={() => setActive(t)}
            >
              {TAB_LABEL[t] ?? t}
            </button>
          ))}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 2 }} title="Text size">
          <button
            className="btn btn-ghost btn-sm"
            style={{ padding: "2px 8px", fontSize: 12 }}
            disabled={sizeIdx === 0}
            onClick={() => setSizeIdx((i) => Math.max(0, i - 1))}
            aria-label="Smaller text"
          >
            A−
          </button>
          <button
            className="btn btn-ghost btn-sm"
            style={{ padding: "2px 8px", fontSize: 15 }}
            disabled={sizeIdx === SIZES.length - 1}
            onClick={() => setSizeIdx((i) => Math.min(SIZES.length - 1, i + 1))}
            aria-label="Larger text"
          >
            A+
          </button>
        </div>
      </div>

      <div className="tab-content">
        {doc ? (
          <>
            {doc.filename && <div className="doc-meta">{doc.filename}</div>}
            {doc.textContent ? (
              <div className="doc-body" style={{ fontSize, lineHeight: 1.6 }}>{doc.textContent}</div>
            ) : (
              <div className="doc-empty">Binary attachment ({doc.mimeType ?? "file"})</div>
            )}
          </>
        ) : (
          <div className="doc-empty">No document</div>
        )}
      </div>
    </div>
  );
}
