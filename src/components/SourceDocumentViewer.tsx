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

  return (
    <div className="left-panel" style={fill ? { flex: 1, width: "auto", minWidth: 0 } : undefined}>
      <div className="cq-block">
        <span className="field-label">Clinical question</span>
        <div className="cq-text">{patient.clinicalQuestion ?? "—"}</div>
      </div>
      <div className="tab-nav">
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
      <div className="tab-content">
        {doc ? (
          <>
            {doc.filename && <div className="doc-meta">{doc.filename}</div>}
            {doc.textContent ? (
              <div className="doc-body">{doc.textContent}</div>
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
