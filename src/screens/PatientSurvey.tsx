import { useState } from "react";
import { useApp } from "../app/AppContext";
import SurveyPanel, { type SurveyQuestion } from "../components/SurveyPanel";

const PER_PATIENT_QUESTIONS: SurveyQuestion[] = [
  {
    id: "ai_quality",
    prompt: "Overall, how clinically appropriate were the AI recommendations for this patient?",
    type: "likert",
    scale: ["Poor", "Excellent"],
  },
  {
    id: "influence",
    prompt: "How much did the AI recommendations influence your final plan?",
    type: "likert",
    scale: ["Not at all", "A great deal"],
  },
  {
    id: "comments",
    prompt: "Any comments on the AI recommendations for this patient? (optional)",
    type: "text",
  },
];

export default function PatientSurvey() {
  const { currentPatient, actions } = useApp();
  const [answers, setAnswers] = useState<Record<string, string>>({});
  if (!currentPatient) return null;

  const required = PER_PATIENT_QUESTIONS.filter((q) => q.type === "likert").map((q) => q.id);
  const complete = required.every((id) => answers[id]);

  return (
    <div className="center-screen">
      <div className="card" style={{ maxWidth: 560 }}>
        <span className="field-label">Per-patient survey</span>
        <h2 style={{ fontSize: 19 }}>{currentPatient.displayLabel}</h2>
        <p style={{ marginBottom: 20 }}>
          Please answer a few short questions about this patient before returning to the queue.
        </p>
        <SurveyPanel
          questions={PER_PATIENT_QUESTIONS}
          answers={answers}
          onChange={(id, v) => setAnswers((a) => ({ ...a, [id]: v }))}
        />
        <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 24 }}>
          <button
            className="btn btn-primary"
            disabled={!complete}
            onClick={() => actions.submitPatientSurvey(currentPatient.id, answers)}
          >
            Save &amp; return to queue
          </button>
        </div>
      </div>
    </div>
  );
}
