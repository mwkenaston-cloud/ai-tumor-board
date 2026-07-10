// Shared domain models for the AI Tumor Board reviewer app.
// These mirror the SQLCipher schema (src-tauri/migrations/001_initial.sql) and
// the prototype's in-memory shapes, but are the authoritative TS contract that
// the typed IPC layer (services/ipc.ts) speaks to the Rust backend.

// ---- Workflow state machines -------------------------------------------------

export type AssignmentState =
  | "locked"
  | "ready"
  | "in_progress"
  | "partially_complete"
  | "complete"
  | "submitted";

export type PatientState =
  | "not_started"
  | "in_progress"
  | "complete"
  | "reopened";

export type Role = "coordinator" | "reviewer";

// ---- Note blocks (physician-authored vs AI-derived stay distinct) -----------

export type NoteBlock =
  | {
      id: string;
      type: "user";
      currentText: string;
      position: number;
    }
  | {
      id: string;
      type: "ai";
      recommendationId: string;
      originalText: string; // immutable — the LLM text as inserted
      currentText: string; // physician-editable
      position: number;
    };

// ---- Recommendations & decisions --------------------------------------------

export type DecisionStatus =
  | "pending"
  | "used"
  | "used-and-edited"
  | "dismissed";

export interface Recommendation {
  id: string;
  patientId: string;
  position: number;
  priorityRank: number | null;
  temperatureLevel: number | null;
  temperatureLabel: string | null;
  evidenceTier: string | null;
  riskScore: number | null;
  safetyScore: number | null;
  title: string | null;
  text: string; // condensed recommendation text
  fullText: string | null;
  rationale: string | null;
  /** contraindications, drug_interactions, adverse_events, monitoring_plan,
   *  epistemic/patient uncertainty, conflicts, bias flags */
  metadata: Record<string, unknown> | null;
  isCustom: boolean;
}

export interface RecommendationDecision {
  recommendationId: string;
  status: DecisionStatus;
  originalText: string | null;
  finalText: string | null;
  editDistance: number | null;
  similarityPercent: number | null;
  decisionElapsedSeconds: number | null;
  dismissalReason: string | null;
  decidedAt: string | null;
}

// ---- Source documents --------------------------------------------------------

export type DocumentType =
  | "notes"
  | "pathology"
  | "imaging"
  | "labs"
  | string;

export interface SourceDocument {
  id: string;
  patientId: string;
  documentType: DocumentType;
  filename: string | null;
  mimeType: string | null;
  /** Present for text documents; binary docs are fetched on demand by id. */
  textContent: string | null;
  byteSize: number | null;
  sha256: string;
}

// ---- Patients & assignment ---------------------------------------------------

export interface Patient {
  id: string;
  researchId: string | null;
  displayLabel: string;
  clinicalQuestion: string | null;
  position: number;
  status: PatientState;
  startedAt: string | null;
  completedAt: string | null;
  elapsedSeconds: number;
  documents: SourceDocument[];
  recommendations: Recommendation[];
  decisions: RecommendationDecision[];
  noteBlocks: NoteBlock[];
}

export interface PatientSummary {
  id: string;
  researchId: string | null;
  displayLabel: string;
  position: number;
  status: PatientState;
  elapsedSeconds: number;
}

export interface Assignment {
  studyId: string;
  studyTitle: string;
  protocolVersion: string;
  schemaVersion: number;
  contactEmail: string | null;
  instructions: string | null;
  settings: StudySettings;
  reviewerId: string;
  reviewerDisplayName: string | null;
  state: AssignmentState;
  patients: PatientSummary[];
}

// ---- Study settings (ported from prototype studySettings) -------------------

export interface StudySettings {
  // identity
  studyTitle?: string;
  piName?: string;
  institution?: string;
  irbNumber?: string;
  studyArm?: string;
  surveyInstructions?: string;
  coiStatement?: string;
  // display toggles
  showPriority?: boolean;
  showEvidence?: boolean;
  showSafety?: boolean;
  showInteractions?: boolean;
  showTemperature?: boolean;
  showDetails?: boolean;
  allowDismiss?: boolean;
  showSynthesis?: boolean;
  // timing
  showTimer?: boolean;
  allowPause?: boolean;
  timeLimitEnabled?: boolean;
  timeLimitMinutes?: number;
  recTimestamps?: boolean;
  // survey
  perPatientSurvey?: boolean;
  surveySkippable?: boolean;
  generalSurvey?: boolean;
  surveyNotes?: boolean;
  // blinding
  anonNames?: boolean;
  randomizeRecs?: boolean;
  hideAILabel?: boolean;
  maskPriority?: boolean;
}

// ---- Audit events ------------------------------------------------------------

export type AuditEventType =
  | "ASSIGNMENT_OPENED"
  | "PASSWORD_ACCEPTED"
  | "PATIENT_OPENED"
  | "PATIENT_COMPLETED"
  | "PATIENT_REOPENED"
  | "DOCUMENT_VIEWED"
  | "RECOMMENDATION_INSERTED"
  | "RECOMMENDATION_EDITED"
  | "RECOMMENDATION_DISMISSED"
  | "CUSTOM_RECOMMENDATION_ADDED"
  | "NOTE_BLOCK_REMOVED"
  | "SURVEY_COMPLETED"
  | "ASSIGNMENT_SUBMITTED";

export interface AuditEvent {
  eventType: AuditEventType;
  patientId?: string | null;
  eventTime: string; // ISO 8601
  payload?: Record<string, unknown>;
}
