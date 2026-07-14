// In-memory seed data for Phase 1 (UI-only). This is SYNTHETIC, non-PHI sample
// content so the reviewer workflow is clickable via `npm run tauri dev` before
// the SQLCipher repository (Phase 2) is wired in. No real patient data here.

import type { Assignment, Patient, StudySettings } from "../models/types";

const settings: StudySettings = {
  studyTitle: "AI Tumor Board",
  showPriority: true,
  showEvidence: true,
  showSafety: true,
  showTemperature: true,
  showDetails: true,
  allowDismiss: true,
  showTimer: true,
  perPatientSurvey: true,
  generalSurvey: true,
};

const patientA: Patient = {
  id: "PT-1",
  researchId: "TUM-0042",
  modelId: "demo-model-v1",
  displayLabel: "Stage III colon adenocarcinoma",
  clinicalQuestion:
    "72yo with resected stage III colon adenocarcinoma (pT3N1). What adjuvant systemic therapy is most appropriate?",
  cancerType: "Stage III colon adenocarcinoma",
  context: null,
  framing: null,
  position: 0,
  status: "not_started",
  startedAt: null,
  completedAt: null,
  elapsedSeconds: 0,
  documents: [
    {
      id: "doc-a-notes",
      patientId: "PT-1",
      documentType: "notes",
      filename: "oncology-note.txt",
      mimeType: "text/plain",
      textContent:
        "HPI: 72yo presents after right hemicolectomy for a T3 lesion of the ascending colon. " +
        "1 of 18 nodes positive. Margins negative. ECOG 0. No significant comorbidities.\n\n" +
        "Assessment: pT3N1 (stage IIIB) colon adenocarcinoma, MSS, resected.",
      byteSize: 240,
      sha256: "seed-a-notes",
    },
    {
      id: "doc-a-path",
      patientId: "PT-1",
      documentType: "pathology",
      filename: "pathology.txt",
      mimeType: "text/plain",
      textContent:
        "Right hemicolectomy: moderately differentiated adenocarcinoma, invades through " +
        "muscularis propria into subserosa (pT3). 1/18 lymph nodes positive. LVI present. " +
        "MMR: proficient (MSS). Margins negative.",
      byteSize: 210,
      sha256: "seed-a-path",
    },
  ],
  recommendations: [
    {
      id: "REC-1",
      patientId: "PT-1",
      position: 0,
      priorityRank: 1,
      temperatureLevel: 2,
      temperatureLabel: "Moderate-Conservative",
      evidenceTier: "I",
      riskScore: 2,
      safetyScore: 88,
      title: "Adjuvant CAPOX x 3 months",
      text:
        "Recommend adjuvant CAPOX (capecitabine + oxaliplatin) for 3 months given stage III, " +
        "MSS disease and ECOG 0, per IDEA-collaboration risk stratification (T3N1 = low risk).",
      fullText:
        "For low-risk stage III colon cancer (T1–3, N1), the IDEA collaboration supports 3 months " +
        "of CAPOX as non-inferior to 6 months with substantially less neurotoxicity. Begin within " +
        "6–8 weeks of surgery.",
      rationale: "IDEA collaboration; low-risk stage III favors shorter oxaliplatin exposure.",
      metadata: {
        monitoring_plan: "CBC, CMP before each cycle; assess neuropathy each visit.",
        drug_interactions: ["Warfarin — capecitabine potentiates INR"],
      },
      isCustom: false,
    },
    {
      id: "REC-2",
      patientId: "PT-1",
      position: 1,
      priorityRank: 3,
      temperatureLevel: 4,
      temperatureLabel: "Aggressive",
      evidenceTier: "II",
      riskScore: 4,
      safetyScore: 62,
      title: "6 months FOLFOX",
      text:
        "Alternative: 6 months of FOLFOX for maximal disease control, accepting higher cumulative " +
        "oxaliplatin neurotoxicity risk.",
      fullText:
        "6 months of oxaliplatin-based therapy remains an option, particularly for higher-risk " +
        "features. Weigh against increased grade 3+ neuropathy.",
      rationale: "Higher cumulative benefit in some subgroups; higher toxicity.",
      metadata: { adverse_events: ["Grade 3 peripheral neuropathy (cumulative)"] },
      isCustom: false,
    },
  ],
  decisions: [],
  noteBlocks: [],
};

const patientB: Patient = {
  id: "PT-2",
  researchId: "TUM-0043",
  modelId: "demo-model-v1",
  displayLabel: "EGFR+ lung adenocarcinoma",
  clinicalQuestion:
    "64yo never-smoker with metastatic EGFR exon 19 del lung adenocarcinoma. First-line systemic therapy?",
  cancerType: "EGFR+ lung adenocarcinoma",
  context: null,
  framing: null,
  position: 1,
  status: "not_started",
  startedAt: null,
  completedAt: null,
  elapsedSeconds: 0,
  documents: [
    {
      id: "doc-b-notes",
      patientId: "PT-2",
      documentType: "notes",
      filename: "note.txt",
      mimeType: "text/plain",
      textContent:
        "64yo never-smoker, metastatic lung adenocarcinoma with EGFR exon 19 deletion on NGS. " +
        "Brain MRI: two small asymptomatic metastases. ECOG 1.",
      byteSize: 160,
      sha256: "seed-b-notes",
    },
  ],
  recommendations: [
    {
      id: "REC-1",
      patientId: "PT-2",
      position: 0,
      priorityRank: 1,
      temperatureLevel: 2,
      temperatureLabel: "Moderate-Conservative",
      evidenceTier: "I",
      riskScore: 2,
      safetyScore: 90,
      title: "First-line osimertinib",
      text:
        "Start osimertinib 80 mg daily as first-line therapy; CNS-active and preferred for EGFR " +
        "exon 19 del, including with asymptomatic brain metastases.",
      fullText:
        "Osimertinib demonstrated superior PFS/OS (FLAURA) and strong CNS activity, making it the " +
        "preferred first-line TKI for EGFR exon 19 del / L858R.",
      rationale: "FLAURA; CNS penetration covers asymptomatic brain mets.",
      metadata: { monitoring_plan: "Baseline + periodic ECG (QTc), LVEF if symptomatic." },
      isCustom: false,
    },
  ],
  decisions: [],
  noteBlocks: [],
};

export function seedPatients(): Record<string, Patient> {
  return { "PT-1": structuredClone(patientA), "PT-2": structuredClone(patientB) };
}

export function seedAssignment(): Assignment {
  return {
    studyId: "STUDY-1",
    studyTitle: settings.studyTitle ?? "AI Tumor Board Study",
    protocolVersion: "v1",
    schemaVersion: 1,
    contactEmail: "coordinator@example.org",
    instructions:
      "Review each assigned patient's source documents and the AI-generated recommendations. " +
      "Compose an independent assessment & plan, inserting or dismissing AI recommendations as " +
      "you see fit. Your edits and decisions are recorded for the study. Complete the short " +
      "survey after each patient.",
    settings,
    reviewerId: "REV-004",
    reviewerDisplayName: "Reviewer 04",
    state: "ready",
    patients: [
      { id: "PT-1", researchId: "TUM-0042", modelId: "demo-model-v1", displayLabel: patientA.displayLabel, position: 0, status: "not_started", elapsedSeconds: 0 },
      { id: "PT-2", researchId: "TUM-0043", modelId: "demo-model-v1", displayLabel: patientB.displayLabel, position: 1, status: "not_started", elapsedSeconds: 0 },
    ],
  };
}
