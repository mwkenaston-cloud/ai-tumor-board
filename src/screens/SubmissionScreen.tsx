import { useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useApp } from "../app/AppContext";
import { attributionMetrics } from "../services/noteBlocks";
import { ipc, isTauri, type ResponseReceipt } from "../services/ipc";

export default function SubmissionScreen() {
  const { assignment, patients } = useApp();
  const [receipt, setReceipt] = useState<ResponseReceipt | null>(null);
  const [exportErr, setExportErr] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  if (!assignment) return null;

  const exportResponse = async () => {
    setExportErr(null);
    const stamp = new Date().toISOString().slice(0, 10);
    const destination = await saveDialog({
      defaultPath: `TumorBoard_Response_${assignment.reviewerId}_${stamp}.atbr`,
      filters: [{ name: "AI Tumor Board Response", extensions: ["atbr"] }],
    });
    if (!destination) return;
    setExporting(true);
    try {
      setReceipt(await ipc.exportResponse(destination));
    } catch (e) {
      setExportErr("Could not create the response file.");
      console.error(e);
    } finally {
      setExporting(false);
    }
  };

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

        {isTauri() && (
          <div style={{ marginTop: 16 }}>
            {receipt ? (
              <div
                style={{
                  background: "#f0fdf4",
                  border: "1px solid #bbf7d0",
                  borderRadius: 10,
                  padding: 14,
                  fontSize: 12,
                  color: "#166534",
                }}
              >
                Response file saved for {receipt.reviewerId} ({receipt.patientCount} patients).
                <div style={{ fontFamily: "var(--font-mono)", fontSize: 10.5, wordBreak: "break-all", marginTop: 6 }}>
                  SHA-256: {receipt.sha256}
                </div>
                <div style={{ marginTop: 6 }}>
                  Return this <code>.atbr</code> file to the study coordinator. Send the file and any
                  password through separate channels.
                </div>
              </div>
            ) : (
              <button
                className="btn btn-primary"
                style={{ width: "100%", justifyContent: "center" }}
                disabled={exporting}
                onClick={exportResponse}
              >
                {exporting ? "Creating response…" : "Save encrypted response file (.atbr)"}
              </button>
            )}
            {exportErr && <div className="form-error">{exportErr}</div>}
          </div>
        )}
      </div>
    </div>
  );
}
