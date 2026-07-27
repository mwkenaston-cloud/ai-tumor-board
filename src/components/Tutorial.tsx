import { useCallback, useEffect, useState, type ReactNode } from "react";

/**
 * A self-contained, multi-step walkthrough of physician mode. Opened from the
 * lobby ("How to use") and shown automatically the first time a reviewer reaches
 * the lobby (tracked in localStorage). No backend dependency — pure UI.
 */

const SEEN_KEY = "atb_tutorial_seen_v1";

export function tutorialAlreadySeen(): boolean {
  try {
    return localStorage.getItem(SEEN_KEY) === "1";
  } catch {
    return false;
  }
}

function markSeen() {
  try {
    localStorage.setItem(SEEN_KEY, "1");
  } catch {
    /* ignore private-mode storage failures */
  }
}

interface Step {
  icon: string;
  title: string;
  body: ReactNode;
  art?: ReactNode;
}

/** A tiny mock of the three-tab bar, highlighting one tab. */
function TabBarArt({ active }: { active: 0 | 1 | 2 }) {
  const labels = ["Patient · Timeline & history", "Decision points & perspectives", "Recommendations & plan"];
  return (
    <div className="tut-art tut-tabbar">
      {labels.map((l, i) => (
        <span key={l} className={`tut-tab ${i === active ? "on" : ""}`}>
          {l}
        </span>
      ))}
    </div>
  );
}

const STEPS: Step[] = [
  {
    icon: "🩺",
    title: "Welcome — here's how this works",
    body: (
      <>
        <p>
          You'll review a short queue of cases. For each one, an AI tumor board has already produced a
          set of recommendations. Your job is to read the case, decide what you agree with, and write
          your own assessment &amp; plan — exactly as you would in practice.
        </p>
        <p>
          Everything runs <strong>offline on this computer</strong> and is encrypted. Nothing you type
          is sent anywhere until you export your finished response and email it back yourself.
        </p>
        <p className="tut-muted">This walkthrough takes about a minute. You can reopen it anytime from “How to use.”</p>
      </>
    ),
  },
  {
    icon: "📋",
    title: "Your patient queue",
    body: (
      <>
        <p>
          The lobby lists every patient assigned to you with a status chip
          (<em>not started</em>, <em>in progress</em>, <em>complete</em>). Click a row and confirm to
          begin — a timer starts quietly in the background (you won't see it; it just records how long
          you spend).
        </p>
        <p>
          You can leave a case and come back; your work and time are saved automatically. The
          <strong> ↺</strong> on a row resets that one patient, and <strong>↺ Reset session</strong> at
          the top starts the whole assignment over. Both ask you to confirm first.
        </p>
      </>
    ),
    art: (
      <div className="tut-art tut-queue">
        <div className="tut-qrow"><span className="tut-qid">P-01</span><span>Model A</span><span className="tut-chip done">complete</span></div>
        <div className="tut-qrow on"><span className="tut-qid">P-02</span><span>Model B</span><span className="tut-chip prog">in progress</span></div>
        <div className="tut-qrow"><span className="tut-qid">P-03</span><span>Model C</span><span className="tut-chip">not started</span></div>
      </div>
    ),
  },
  {
    icon: "🗂️",
    title: "Three tabs per case",
    body: (
      <>
        <p>Inside a case, your work is organized into three tabs across the top:</p>
        <ol className="tut-list">
          <li><strong>Patient · Timeline &amp; history</strong> — the clinical picture and the source documents.</li>
          <li><strong>Decision points &amp; perspectives</strong> — the key questions and specialist viewpoints.</li>
          <li><strong>Recommendations &amp; plan</strong> — the AI recommendations and your writing area.</li>
        </ol>
        <p className="tut-muted">Move between them freely; the time you spend on each is recorded for the study.</p>
      </>
    ),
    art: <TabBarArt active={0} />,
  },
  {
    icon: "📄",
    title: "Tab 1 — Timeline, history & source data",
    body: (
      <>
        <p>
          The left side toggles between the <strong>clinical timeline</strong> and the
          <strong> relevant history</strong> (comorbidities, prior treatment, genetics). The right side
          holds the raw source documents — imaging, clinical notes, pathology, and labs — so you can
          read the primary data yourself, not just a summary.
        </p>
      </>
    ),
    art: <TabBarArt active={0} />,
  },
  {
    icon: "🧭",
    title: "Tab 2 — Decision points & perspectives",
    body: (
      <>
        <p>
          This tab lays out the specific decisions the board weighed and the differing specialist
          perspectives on each. It's context for your own judgment — there's nothing to fill in here.
        </p>
      </>
    ),
    art: <TabBarArt active={1} />,
  },
  {
    icon: "✅",
    title: "Tab 3 — Recommendations",
    body: (
      <>
        <p>
          Recommendations are listed in the AI's priority order. Each shows scores for
          <strong> priority</strong>, <strong>evidence</strong>, <strong>risk</strong>, and
          <strong> safety</strong> — <em>hover any score</em> to see the reasoning behind it.
        </p>
        <p>
          <strong>Insert</strong> adds that recommendation's text into your plan; <strong>Dismiss</strong>{" "}
          sets it aside. Both are toggles — click again to undo. Whatever you accept, dismiss, or leave
          untouched is recorded.
        </p>
      </>
    ),
    art: (
      <div className="tut-art tut-rec">
        <div className="tut-rec-head">
          <span className="tut-badge">Priority 1</span>
          <span className="tut-scores"><span>Evidence</span><span>Risk</span><span>Safety</span></span>
        </div>
        <div className="tut-rec-text">Proceed with… <span className="tut-muted">(hover a score for its rationale)</span></div>
        <div className="tut-rec-actions"><span className="tut-btn ins">Insert</span><span className="tut-btn dis">Dismiss</span></div>
      </div>
    ),
  },
  {
    icon: "✍️",
    title: "Tab 3 — Writing your assessment & plan",
    body: (
      <>
        <p>
          Your note lives in the <strong>Assessment &amp; plan</strong> area beside the
          recommendations. Text you insert from a recommendation appears as an editable block — you can
          rewrite it freely, and the app quietly tracks how much you changed it. Type your own
          paragraphs anywhere, and use <strong>+ Add paragraph</strong> for more space.
        </p>
        <p className="tut-muted">Everything autosaves as you go — there's no save button to remember.</p>
      </>
    ),
    art: <TabBarArt active={2} />,
  },
  {
    icon: "🏁",
    title: "Finishing a patient",
    body: (
      <>
        <p>
          When you're done with a case, click <strong>Complete patient</strong>. You'll answer a few
          short survey questions tailored to what you accepted, dismissed, or ignored, and then you're
          returned to the queue for the next patient.
        </p>
        <p>Reopening a completed patient is fine — your answers are kept and you can revise.</p>
      </>
    ),
  },
  {
    icon: "📤",
    title: "Submitting your assignment",
    body: (
      <>
        <p>
          Once <strong>every</strong> patient is complete, <strong>Finish &amp; submit assignment</strong>{" "}
          unlocks. You'll complete one final survey, and the app automatically saves an encrypted
          response file to your Downloads folder.
        </p>
        <p>
          The last step is simple: <strong>email that file back</strong> to the study team using the
          address shown on the confirmation screen. That's it — you're done.
        </p>
      </>
    ),
  },
];

export default function Tutorial({ onClose }: { onClose: () => void }) {
  const [i, setI] = useState(0);
  const last = STEPS.length - 1;
  const step = STEPS[i];

  const close = useCallback(() => {
    markSeen();
    onClose();
  }, [onClose]);

  const next = useCallback(() => setI((n) => Math.min(last, n + 1)), [last]);
  const prev = useCallback(() => setI((n) => Math.max(0, n - 1)), []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
      else if (e.key === "ArrowRight") next();
      else if (e.key === "ArrowLeft") prev();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close, next, prev]);

  return (
    <div className="tut-overlay" onClick={close}>
      <div className="tut-modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-label="Physician mode tutorial">
        <button className="tut-x" onClick={close} aria-label="Close tutorial">✕</button>

        <div className="tut-head">
          <span className="tut-icon">{step.icon}</span>
          <div>
            <div className="tut-step-count">Step {i + 1} of {STEPS.length}</div>
            <h2 className="tut-title">{step.title}</h2>
          </div>
        </div>

        <div className="tut-body">
          {step.body}
          {step.art}
        </div>

        <div className="tut-dots">
          {STEPS.map((_, idx) => (
            <button
              key={idx}
              className={`tut-dot ${idx === i ? "on" : ""}`}
              aria-label={`Go to step ${idx + 1}`}
              onClick={() => setI(idx)}
            />
          ))}
        </div>

        <div className="tut-footer">
          <button className="btn btn-ghost btn-sm" onClick={close}>Skip</button>
          <div style={{ display: "flex", gap: 8 }}>
            {i > 0 && <button className="btn btn-ghost btn-sm" onClick={prev}>← Back</button>}
            {i < last ? (
              <button className="btn btn-primary btn-sm" onClick={next}>Next →</button>
            ) : (
              <button className="btn btn-success btn-sm" onClick={close}>Got it — let's start</button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
