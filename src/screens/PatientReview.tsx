import { useApp } from "../app/AppContext";
import SourceDocumentViewer from "../components/SourceDocumentViewer";
import RecommendationCard from "../components/RecommendationCard";
import NoteEditor from "../components/NoteEditor";
import { attributionMetrics } from "../services/noteBlocks";
import type { NoteBlock } from "../models/types";

export default function PatientReview() {
  const { currentPatient, assignment, actions } = useApp();
  if (!currentPatient || !assignment) return null;
  const p = currentPatient;
  const settings = assignment.settings;

  const decisionFor = (recId: string) =>
    p.decisions.find((d) => d.recommendationId === recId);

  const metrics = attributionMetrics(p.noteBlocks);

  const onChangeBlock = (id: string, text: string) => {
    const next: NoteBlock[] = p.noteBlocks.map((b) =>
      b.id === id ? ({ ...b, currentText: text } as NoteBlock) : b
    );
    actions.setNoteBlocks(p.id, next);
  };

  return (
    <>
      <SourceDocumentViewer patient={p} />

      <div className="middle-panel">
        <div className="recs-header">
          <h2>AI recommendations</h2>
          <span className="recs-count">{p.recommendations.length}</span>
        </div>
        <div className="recs-container">
          {p.recommendations.map((rec) => (
            <RecommendationCard
              key={rec.id}
              rec={rec}
              decision={decisionFor(rec.id)}
              settings={settings}
              onInsert={() => actions.insertRecommendation(p.id, rec.id)}
              onDismiss={() => actions.dismissRecommendation(p.id, rec.id)}
            />
          ))}
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
        <div
          style={{
            display: "flex",
            gap: 8,
            justifyContent: "space-between",
            padding: "10px 16px",
            borderTop: "1px solid var(--border-2)",
            background: "var(--surface-2)",
          }}
        >
          <button className="btn btn-ghost btn-sm" onClick={actions.backToLobby}>
            ← Back to queue
          </button>
          <button className="btn btn-success btn-sm" onClick={() => actions.completePatient(p.id)}>
            Complete patient
          </button>
        </div>
      </div>
    </>
  );
}
