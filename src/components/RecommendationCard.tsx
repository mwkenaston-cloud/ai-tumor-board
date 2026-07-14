import { useState } from "react";
import type {
  Recommendation,
  RecommendationDecision,
  StudySettings,
} from "../models/types";
import HoverInfo from "./HoverInfo";

function safetyClass(score: number | null): string {
  if (score == null) return "safety-mid";
  if (score >= 80) return "safety-high";
  if (score >= 60) return "safety-mid";
  return "safety-low";
}

const EVIDENCE_DEF: Record<string, string> = {
  I: "Evidence Tier I — high-quality RCTs, meta-analyses, or major guideline endorsements (NCCN/ESMO/ASCO).",
  II: "Evidence Tier II — well-designed observational studies, phase II trials, or guidelines with caveats.",
  III: "Evidence Tier III — case series, registry data, expert consensus, or extrapolation from adjacent settings.",
  IV: "Evidence Tier IV — preclinical data, mechanistic rationale, expert opinion, or compassionate use.",
};

const RISK_DEF: Record<number, string> = {
  1: "Risk 1 — minimal risk; widely tolerated with well-characterized toxicity.",
  2: "Risk 2 — low–moderate risk; manageable toxicities in most patients.",
  3: "Risk 3 — moderate risk; requires close monitoring and patient selection.",
  4: "Risk 4 — high risk; significant toxicity/procedural risk; shared decision-making.",
  5: "Risk 5 — very high risk; experimental/aggressive; life-threatening toxicity possible.",
};

function metaStr(meta: Record<string, unknown> | null, key: string): string | null {
  const v = meta?.[key];
  return typeof v === "string" && v.trim().length > 0 ? v : null;
}

interface Props {
  rec: Recommendation;
  decision?: RecommendationDecision;
  settings: StudySettings;
  onInsert: () => void;
  onDismiss: () => void;
  onUndoInsert: () => void;
  onUndoDismiss: () => void;
}

export default function RecommendationCard({
  rec,
  decision,
  settings,
  onInsert,
  onDismiss,
  onUndoInsert,
  onUndoDismiss,
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

  const monitoring = metaStr(rec.metadata, "monitoring_plan");
  const safetyRationale = metaStr(rec.metadata, "safety_score_rationale");
  const priorityRationale = metaStr(rec.metadata, "priority_rationale");

  const priorityTip =
    priorityRationale ??
    `Priority ${rec.priorityRank} — the board's integrated ranking (1 = highest), distinct from temperature level.`;
  const evidenceTip = rec.evidenceTier
    ? EVIDENCE_DEF[rec.evidenceTier] ?? `Evidence tier ${rec.evidenceTier}.`
    : "";
  const riskTip = rec.riskScore != null ? RISK_DEF[Math.round(rec.riskScore)] ?? "" : "";
  const safetyTip =
    rec.safetyScore != null
      ? `Safety ${rec.safetyScore}% — composite probability of avoiding serious adverse events.${safetyRationale ? " " + safetyRationale : ""}`
      : "";

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
          <HoverInfo className={`prio-badge prio-${rec.priorityRank}`} title="Priority" tip={priorityTip}>
            Priority {rec.priorityRank}
          </HoverInfo>
        )}
        {settings.showTemperature && rec.temperatureLevel != null && (
          <span className={`temp-label temp-${rec.temperatureLevel}`}>
            {rec.temperatureLabel ?? `T${rec.temperatureLevel}`}
          </span>
        )}
      </div>

      <div className="score-row">
        {settings.showEvidence && rec.evidenceTier && (
          <HoverInfo className={`score-badge ev-${rec.evidenceTier}`} title="Evidence tier" tip={evidenceTip}>
            Evidence {rec.evidenceTier}
          </HoverInfo>
        )}
        {rec.riskScore != null && (
          <HoverInfo className={`score-badge risk-${Math.round(rec.riskScore)}`} title="Risk score" tip={riskTip}>
            Risk {rec.riskScore}
          </HoverInfo>
        )}
        {settings.showSafety && rec.safetyScore != null && (
          <HoverInfo className={`score-badge ${safetyClass(rec.safetyScore)}`} title="Safety score" tip={safetyTip}>
            Safety {rec.safetyScore}%
          </HoverInfo>
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
        {status === "used" || status === "used-and-edited" ? (
          <button className="btn btn-warning btn-sm" onClick={onUndoInsert}>
            Remove from note
          </button>
        ) : status === "dismissed" ? (
          <button className="btn btn-ghost btn-sm" onClick={onUndoDismiss}>
            Undo dismiss
          </button>
        ) : (
          <>
            <button className="btn btn-primary btn-sm" onClick={onInsert}>
              Insert into note
            </button>
            {settings.allowDismiss && (
              <button className="btn btn-ghost btn-sm" onClick={onDismiss}>
                Dismiss
              </button>
            )}
          </>
        )}
      </div>
    </div>
  );
}
