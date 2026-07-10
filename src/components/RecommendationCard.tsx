import { useState } from "react";
import type {
  Recommendation,
  RecommendationDecision,
  StudySettings,
} from "../models/types";

function safetyClass(score: number | null): string {
  if (score == null) return "safety-mid";
  if (score >= 80) return "safety-high";
  if (score >= 60) return "safety-mid";
  return "safety-low";
}

interface Props {
  rec: Recommendation;
  decision?: RecommendationDecision;
  settings: StudySettings;
  onInsert: () => void;
  onDismiss: () => void;
}

export default function RecommendationCard({
  rec,
  decision,
  settings,
  onInsert,
  onDismiss,
}: Props) {
  const [open, setOpen] = useState(false);
  const status = decision?.status ?? "pending";
  const cardClass =
    status === "used"
      ? "used"
      : status === "used-and-edited"
        ? "used-and-edited"
        : status === "dismissed"
          ? "dismissed"
          : "";

  const monitoring = (rec.metadata?.monitoring_plan as string | undefined) ?? null;

  return (
    <div className={`rec-card ${cardClass}`}>
      {status !== "pending" && (
        <span
          className={`status-tag ${
            status === "used"
              ? "status-used"
              : status === "used-and-edited"
                ? "status-edited"
                : "status-dismissed"
          }`}
        >
          {status.replace(/-/g, " ")}
        </span>
      )}

      <div className="priority-banner">
        <span className="rec-id-label">{rec.title ?? rec.id}</span>
        {settings.showPriority && rec.priorityRank != null && (
          <span className={`prio-badge prio-${rec.priorityRank}`}>
            Priority {rec.priorityRank}
          </span>
        )}
        {settings.showTemperature && rec.temperatureLevel != null && (
          <span className={`temp-label temp-${rec.temperatureLevel}`}>
            {rec.temperatureLabel ?? `T${rec.temperatureLevel}`}
          </span>
        )}
      </div>

      <div className="score-row">
        {settings.showEvidence && rec.evidenceTier && (
          <span className={`score-badge ev-${rec.evidenceTier}`}>
            Evidence {rec.evidenceTier}
          </span>
        )}
        {rec.riskScore != null && (
          <span className={`score-badge risk-${Math.round(rec.riskScore)}`}>
            Risk {rec.riskScore}
          </span>
        )}
        {settings.showSafety && rec.safetyScore != null && (
          <span className={`score-badge ${safetyClass(rec.safetyScore)}`}>
            Safety {rec.safetyScore}%
          </span>
        )}
      </div>

      <div className="rec-text">{rec.text}</div>

      {settings.showDetails && (rec.fullText || rec.rationale || monitoring) && (
        <>
          <button className="rec-detail-toggle" onClick={() => setOpen((o) => !o)}>
            {open ? "▾ Hide details" : "▸ Details"}
          </button>
          {open && (
            <div className="rec-detail-body">
              {rec.fullText && <div className="rec-full-text">{rec.fullText}</div>}
              {rec.rationale && (
                <div style={{ fontSize: 11.5, color: "var(--muted)", marginBottom: 6 }}>
                  <strong>Rationale:</strong> {rec.rationale}
                </div>
              )}
              {monitoring && (
                <div style={{ fontSize: 11.5, color: "#166534" }}>
                  <strong>Monitoring:</strong> {monitoring}
                </div>
              )}
            </div>
          )}
        </>
      )}

      <div className="rec-actions">
        <button className="btn btn-primary btn-sm" onClick={onInsert}>
          Insert into note
        </button>
        {settings.allowDismiss && status !== "dismissed" && (
          <button className="btn btn-ghost btn-sm" onClick={onDismiss}>
            Dismiss
          </button>
        )}
      </div>
    </div>
  );
}
