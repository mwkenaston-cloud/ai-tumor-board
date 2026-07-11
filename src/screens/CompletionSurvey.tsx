import { useState } from "react";
import { useApp } from "../app/AppContext";
import SurveyPanel, { type SurveyQuestion } from "../components/SurveyPanel";

const GENERAL_QUESTIONS: SurveyQuestion[] = [
  { id: "trust", prompt: "How much did you trust the AI recommendations overall?", type: "likert", scale: ["Not at all", "Completely"] },
  { id: "workflow", prompt: "How well would this tool integrate into your workflow?", type: "likert", scale: ["Very poorly", "Very well"] },
  { id: "bias", prompt: "Did you notice bias or safety concerns in the recommendations?", type: "likert", scale: ["None", "Serious"] },
  { id: "primary_concern", prompt: "What is your primary concern about AI-assisted tumor board recommendations?", type: "text" },
  { id: "additional", prompt: "Any additional comments? (optional)", type: "text" },
];

export default function CompletionSurvey() {
  const { actions } = useApp();
  const [answers, setAnswers] = useState<Record<string, string>>({});

  const required = GENERAL_QUESTIONS.filter((q) => q.type === "likert").map((q) => q.id);
  const complete = required.every((id) => answers[id]);

  return (
    <div className="center-screen">
      <div className="card" style={{ maxWidth: 600 }}>
        <span className="field-label">Final survey</span>
        <h2 style={{ fontSize: 20 }}>Before you submit</h2>
        <p style={{ marginBottom: 20 }}>
          You have completed all assigned patients. Please answer these final questions; your
          responses are included in the encrypted submission.
        </p>
        <SurveyPanel
          questions={GENERAL_QUESTIONS}
          answers={answers}
          onChange={(id, v) => setAnswers((a) => ({ ...a, [id]: v }))}
        />
        <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 24 }}>
          <button
            className="btn btn-success"
            disabled={!complete}
            onClick={() => {
              if (
                window.confirm(
                  "Submit your completed assignment? This finalizes your responses and generates the encrypted response file to return to the study team."
                )
              ) {
                actions.submitAssignment(answers);
              }
            }}
          >
            Submit assignment
          </button>
        </div>
      </div>
    </div>
  );
}
