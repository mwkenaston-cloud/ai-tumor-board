# User guide

Two roles, one app. Everything runs offline.

## Coordinator

Open the app → **Coordinator** → password **`edit`**.

**Build & assign tab**
1. **Add patients** (Research ID + Model ID). Cancer type and clinical question
   are filled in from the AI import.
2. Select a patient → **Upload combined source file** (one `.txt` split by the
   headers `Txt Imaging`, `Txt Clinical Notes`, `Txt Pathology`, `Txt Labs`).
3. Select the patient → paste the **AI output JSON** → **Validate & import**.

**Reviewers tab**
4. **Add a reviewer** (ID + name). Toggle the patients/models to assign them.
5. **Generate package** for that reviewer → enter an assignment password → save
   the `.atb`. (You can build multiple batches per reviewer.)
6. **Delete a reviewer** removes them and all their batches/responses.

**Responses & results tab**
7. **Import response** — select a returned `.atbr`. Duplicates are rejected.
8. See per-recommendation accept/dismiss/not-used aggregated across physicians,
   average % physician-authored, and which reviewers responded.
9. **Delete** an uploaded response (✕ on a responded batch) if needed.
10. **Export analysis** → writes a JSON + CSV with all manuscript variables for
    pooled analysis.

Send each reviewer their `.atb` **and** its password through **separate**
channels.

## Physician (reviewer)

Open the app → **Physician** → choose the `.atb` file → enter the password.

- **Queue:** click a patient → confirm **Begin review** (starts the timer).
  Reset a single patient (↺) or the whole session; leave to **Home** anytime.
- **Patient tab:** toggle **Clinical timeline** ↔ **Relevant history** (CCI,
  treatment-relevant flags, comorbidities, family history/genetics); raw source
  documents are on the right.
- **Decision points & perspectives tab:** the AI's framing.
- **Recommendations & plan tab:** recommendation cards (hover any score badge for
  its rationale/uncertainty) on 2/3, your Assessment & Plan editor on 1/3.
  **Insert** a recommendation as a tracked block or **Dismiss** it — both toggle
  (Remove / Undo). Your edits, timing, and decisions are recorded.
- **Complete patient** → per-recommendation survey → back to the queue.
- When all patients are done → **Finish & submit** → final survey → confirm.
  The encrypted `.atbr` is **auto-saved to your Downloads folder**; email it back
  to the study contact shown on screen.

Your work autosaves continuously and survives closing/reopening the app.
