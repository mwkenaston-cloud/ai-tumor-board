import { useEffect, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useApp } from "../app/AppContext";
import { attributionMetrics } from "../services/noteBlocks";
import { ipc, isTauri, type ExportedResponse } from "../services/ipc";

export default function SubmissionScreen() {
  const { assignment, patients, actions } = useApp();
  const [exported, setExported] = useState<ExportedResponse | null>(null);
  const [exportErr, setExportErr] = useState<string | null>(null);
  const started = useRef(false);

  // On arrival, automatically write the sealed response to the Downloads folder.
  useEffect(() => {
    if (!isTauri() || started.current) return;
    started.current = true;
    ipc
      .exportResponseToDownloads()
      .then(setExported)
      .catch((e) => {
        console.error(e);
        setExportErr("Could not automatically save the response file. Use ‘Save a copy’ below.");
      });
  }, []);

  const saveCopy = async () => {
    setExportErr(null);
    const stamp = new Date().toISOString().slice(0, 10);
    const destination = await saveDialog({
      defaultPath: exported?.filename ?? `TumorBoard_Response_${assignment?.reviewerId}_${stamp}.atbr`,
      filters: [{ name: "AI Tumor Board Response", extensions: ["atbr"] }],
    });
    if (!destination) return;
    try {
      await ipc.exportResponse(destination);
    } catch (e) {
      setExportErr("Could not save a copy.");
      console.error(e);
    }
  };

  if (!assignment) return null;
  const list = Object.values(patients);
  const totalRecs = list.reduce((n, p) => n + p.recommendations.length, 0);
  const used = list.reduce((n, p) => n + p.decisions.filter((d) => d.status.startsWith("used")).length, 0);
  const dismissed = list.reduce((n, p) => n + p.decisions.filter((d) => d.status === "dismissed").length, 0);
  const contact = assignment.contactEmail;

  return (
    <div className="center-screen">
      <div className="card" style={{ maxWidth: 580, textAlign: "center" }}>
        <div style={{ fontSize: 40 }}>✅</div>
        <h2>Assignment submitted</h2>
        <p>
          Your responses for <strong>{assignment.studyTitle}</strong> are complete. This assignment
          is now read-only.
        </p>

        {isTauri() && (
          <div
            style={{
              textAlign: "left",
              background: exported ? "#f0fdf4" : "var(--surface-2)",
              border: `1px solid ${exported ? "#bbf7d0" : "var(--border-2)"}`,
              borderRadius: 10,
              padding: 16,
              marginBottom: 14,
            }}
          >
            {exported ? (
              <>
                <div style={{ fontWeight: 700, color: "#166534", marginBottom: 6 }}>
                  📄 Response file saved to your Downloads folder
                </div>
                <div style={{ fontFamily: "var(--font-mono)", fontSize: 11, color: "var(--text-2)", wordBreak: "break-all" }}>
                  {exported.path}
                </div>
                <div style={{ marginTop: 12, fontSize: 13.5, color: "#1e293b", lineHeight: 1.6 }}>
                  <strong>Please email this file back to the study team</strong>
                  {contact ? (
                    <>
                      {" "}at{" "}
                      <a href={`mailto:${contact}?subject=${encodeURIComponent("Tumor Board Response " + exported.filename)}`} style={{ color: "var(--primary)", fontWeight: 600 }}>
                        {contact}
                      </a>
                      .
                    </>
                  ) : (
                    " using the address provided by your coordinator."
                  )}{" "}
                  Attach <code>{exported.filename}</code> to the message.
                </div>
                <div style={{ fontSize: 10.5, fontFamily: "var(--font-mono)", color: "var(--muted-2)", marginTop: 8, wordBreak: "break-all" }}>
                  SHA-256: {exported.sha256}
                </div>
              </>
            ) : (
              <div style={{ fontSize: 13, color: "var(--muted)" }}>Saving encrypted response file…</div>
            )}
            {exportErr && <div className="form-error">{exportErr}</div>}
            <button className="btn btn-ghost btn-sm" style={{ marginTop: 10 }} onClick={saveCopy}>
              Save a copy elsewhere…
            </button>
          </div>
        )}

        <div style={{ textAlign: "left", background: "var(--surface-2)", border: "1px solid var(--border-2)", borderRadius: 10, padding: 16 }}>
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

        <button className="btn btn-ghost" style={{ marginTop: 16 }} onClick={actions.goHome}>
          ← Return to home
        </button>
      </div>
    </div>
  );
}
