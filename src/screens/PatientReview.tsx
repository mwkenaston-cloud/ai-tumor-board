import { useEffect, useRef, useState } from "react";
import { useApp } from "../app/AppContext";
import PatientContextView from "../components/PatientContextView";
import FramingView from "../components/FramingView";
import RecommendationCard from "../components/RecommendationCard";
import NoteEditor from "../components/NoteEditor";
import { attributionMetrics } from "../services/noteBlocks";
import type { NoteBlock } from "../models/types";

type Tab = "patient" | "framing" | "recommendations";

const TABS: { id: Tab; label: string }[] = [
  { id: "patient", label: "Patient · Timeline & history" },
  { id: "framing", label: "Decision points & perspectives" },
  { id: "recommendations", label: "Recommendations & plan" },
];

export default function PatientReview() {
  const { currentPatient, assignment, actions } = useApp();
  const [tab, setTab] = useState<Tab>("patient");

  // Tab dwell-time tracking for engagement trajectories.
  const tabEnteredAt = useRef(Date.now());
  const currentTab = useRef<Tab>("patient");
  const pid = currentPatient?.id ?? null;

  const changeTab = (next: Tab) => {
    const seconds = Math.round((Date.now() - tabEnteredAt.current) / 1000);
    actions.logEvent(pid, "TAB_VIEWED", { tab: currentTab.current, seconds, next });
    tabEnteredAt.current = Date.now();
    currentTab.current = next;
    setTab(next);
  };

  // On leaving the review, record time spent on the final tab.
  useEffect(() => {
    return () => {
      const seconds = Math.round((Date.now() - tabEnteredAt.current) / 1000);
      actions.logEvent(pid, "TAB_VIEWED", { tab: currentTab.current, seconds, next: null });
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pid]);

  if (!currentPatient || !assignment) return null;
  const p = currentPatient;
  // Guard against a study with no settings (e.g., an older package): default to
  // showing everything rather than crashing on undefined toggles.
  const settings =
    assignment.settings ?? ({
      showPriority: true,
      showEvidence: true,
      showSafety: true,
      showTemperature: true,
      showDetails: true,
      allowDismiss: true,
    } as typeof assignment.settings);

  const decisionFor = (recId: string) => p.decisions.find((d) => d.recommendationId === recId);
  const metrics = attributionMetrics(p.noteBlocks);

  // Present recommendations in the board's priority order (1 = highest);
  // fall back to LLM output order for any without a rank.
  const orderedRecs = [...p.recommendations].sort(
    (a, b) => (a.priorityRank ?? 999) - (b.priorityRank ?? 999) || a.position - b.position
  );
  // The AI's single highest-ranked recommendation gets a "most applicable" badge
  // (only when a real priority ranking exists).
  const topPickId = orderedRecs[0]?.priorityRank != null ? orderedRecs[0].id : null;

  const onChangeBlock = (id: string, text: string) => {
    const next: NoteBlock[] = p.noteBlocks.map((b) =>
      b.id === id ? ({ ...b, currentText: text } as NoteBlock) : b
    );
    actions.setNoteBlocks(p.id, next);
  };

  const complete = () => {
    if (window.confirm(`Complete and stop reviewing ${p.researchId ?? p.id}? The timer will stop.`)) {
      actions.completePatient(p.id);
    }
  };

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
      {/* Tab bar */}
      <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "6px 14px", borderBottom: "1px solid var(--border)", background: "var(--surface)", flexShrink: 0 }}>
        <span style={{ fontSize: 12, color: "var(--muted)", marginRight: 8 }}>
          {p.researchId ?? p.id}{p.cancerType && ` · ${p.cancerType}`}
        </span>
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`btn btn-sm ${tab === t.id ? "btn-primary" : "btn-ghost"}`}
            onClick={() => changeTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
        {tab === "patient" && <PatientContextView patient={p} />}
        {tab === "framing" && <FramingView patient={p} />}
        {tab === "recommendations" && (
          <>
            <div className="middle-panel">
              <div className="recs-header">
                <h2>AI recommendations</h2>
                <span className="recs-count">{p.recommendations.length}</span>
              </div>
              <p style={{ fontSize: 11.5, color: "var(--muted)", lineHeight: 1.5, margin: "0 0 10px" }}>
                Listed in the AI's priority order (1 = highest). You can <strong>insert several</strong>{" "}
                into your plan, <strong>edit</strong> the inserted text however you like, and{" "}
                <strong>dismiss</strong> any you disagree with — each is reversible. Hover a priority,
                evidence, risk, or safety score to see the reasoning behind it.
              </p>
              <div className="recs-container">
                {orderedRecs.map((rec) => (
                  <RecommendationCard
                    key={rec.id}
                    rec={rec}
                    decision={decisionFor(rec.id)}
                    settings={settings}
                    isTopPick={rec.id === topPickId}
                    onInsert={() => actions.insertRecommendation(p.id, rec.id)}
                    onDismiss={() => actions.dismissRecommendation(p.id, rec.id)}
                    onUndoInsert={() => actions.undoInsert(p.id, rec.id)}
                    onUndoDismiss={() => actions.undoDismiss(p.id, rec.id)}
                  />
                ))}
                {p.recommendations.length === 0 && (
                  <div className="doc-empty">No AI recommendations were imported for this patient.</div>
                )}
              </div>
            </div>
            <div className="right-panel">
              <div className="note-header">
                <h3>Assessment &amp; plan</h3>
                <span className="note-meta">
                  {metrics.wordCount} words · {metrics.pctDerivedFromLlm}% from AI
                </span>
              </div>
              <NoteEditor
                blocks={p.noteBlocks}
                onChangeBlock={onChangeBlock}
                onRemoveBlock={(id) => actions.removeBlock(p.id, id)}
                onAddParagraph={() => actions.appendUserBlock(p.id)}
              />
            </div>
          </>
        )}
      </div>

      {/* Persistent footer */}
      <div style={{ display: "flex", gap: 8, justifyContent: "space-between", padding: "10px 16px", borderTop: "1px solid var(--border-2)", background: "var(--surface-2)", flexShrink: 0 }}>
        <button className="btn btn-ghost btn-sm" onClick={actions.backToLobby}>← Back to queue</button>
        <button className="btn btn-success btn-sm" onClick={complete}>Complete patient</button>
      </div>
    </div>
  );
}
