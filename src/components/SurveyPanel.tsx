export interface SurveyQuestion {
  id: string;
  prompt: string;
  type: "likert" | "text";
  /** Optional endpoint labels for a likert scale, e.g. ["Not at all","Extremely"]. */
  scale?: [string, string];
}

interface Props {
  questions: SurveyQuestion[];
  answers: Record<string, string>;
  onChange: (id: string, value: string) => void;
}

/** Renders a small survey form. Purely controlled; parent owns the answers. */
export default function SurveyPanel({ questions, answers, onChange }: Props) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      {questions.map((q) => (
        <div key={q.id}>
          <label className="field-label" style={{ textTransform: "none", fontSize: 13, color: "#1e293b" }}>
            {q.prompt}
          </label>
          {q.type === "likert" ? (
            <div>
              <div style={{ display: "flex", gap: 6 }}>
                {[1, 2, 3, 4, 5].map((n) => {
                  const active = answers[q.id] === String(n);
                  return (
                    <button
                      key={n}
                      className={`btn btn-sm ${active ? "btn-primary" : "btn-ghost"}`}
                      style={{ width: 40, justifyContent: "center" }}
                      onClick={() => onChange(q.id, String(n))}
                    >
                      {n}
                    </button>
                  );
                })}
              </div>
              {q.scale && (
                <div style={{ display: "flex", justifyContent: "space-between", fontSize: 10.5, color: "var(--muted-2)", marginTop: 4, maxWidth: 236 }}>
                  <span>{q.scale[0]}</span>
                  <span>{q.scale[1]}</span>
                </div>
              )}
            </div>
          ) : (
            <textarea
              className="text-input"
              style={{ minHeight: 70, resize: "vertical", fontSize: 13 }}
              value={answers[q.id] ?? ""}
              onChange={(e) => onChange(q.id, e.currentTarget.value)}
            />
          )}
        </div>
      ))}
    </div>
  );
}
