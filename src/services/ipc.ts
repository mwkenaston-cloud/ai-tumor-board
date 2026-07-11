// Typed wrappers over the Rust command surface. The UI never invokes strings
// directly. Tauri converts camelCase JS keys to snake_case Rust params, and the
// Rust DTOs serialize as camelCase, so these line up with models/types.ts.

import { invoke } from "@tauri-apps/api/core";
import type {
  Assignment,
  NoteBlock,
  Patient,
  RecommendationDecision,
} from "../models/types";

/** True when running inside the Tauri shell (vs a plain browser dev preview). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const ipc = {
  openDevAssignment: () => invoke<Assignment>("open_dev_assignment"),
  openAssignment: (path: string, password: string) =>
    invoke<Assignment>("open_assignment", { path, password }),
  loadAssignment: () => invoke<Assignment>("load_assignment"),
  getPatient: (patientId: string) => invoke<Patient>("get_patient", { patientId }),
  openPatient: (patientId: string) => invoke<Patient>("open_patient", { patientId }),
  saveNoteBlocks: (patientId: string, blocks: NoteBlock[]) =>
    invoke<void>("save_note_blocks", { patientId, blocks }),
  saveDecision: (decision: RecommendationDecision) =>
    invoke<void>("save_decision", { decision }),
  completePatient: (patientId: string, elapsedSeconds: number) =>
    invoke<void>("complete_patient", { patientId, elapsedSeconds }),
  saveSurvey: (patientId: string | null, answers: Record<string, string>) =>
    invoke<void>("save_survey", { patientId, answers }),
  submitAssignment: (generalAnswers: Record<string, string>) =>
    invoke<void>("submit_assignment", { generalAnswers }),
  exportResponse: (destination: string) =>
    invoke<ResponseReceipt>("export_response", { destination }),
  cipherVersion: () => invoke<string | null>("cipher_version"),
};

export interface ResponseReceipt {
  sha256: string;
  reviewerId: string;
  patientCount: number;
}

// ── Coordinator ──────────────────────────────────────────────────────────
export interface CoordPatient {
  id: string;
  researchId: string | null;
  modelId: string | null;
  cancerType: string | null;
  clinicalQuestion: string | null;
  documentCount: number;
  recommendationCount: number;
}
export interface CoordReviewer {
  reviewerId: string;
  displayName: string | null;
  patientCount: number;
}
export interface CoordinatorSummary {
  studyTitle: string;
  provisioned: boolean;
  patients: CoordPatient[];
  reviewers: CoordReviewer[];
  resultsCount: number;
}
export interface PackageReceipt {
  sha256: string;
  patientCount: number;
  reviewerId: string;
  assignmentId: string;
}
export interface ImportSummary {
  reviewerId: string;
  assignmentId: string;
  patientCount: number;
}
export interface ResultRow {
  assignmentId: string;
  reviewerId: string;
  submittedAt: string | null;
  importedAt: string;
}

export const coordinatorIpc = {
  openWorkspace: () => invoke<CoordinatorSummary>("coordinator_open_workspace"),
  summary: () => invoke<CoordinatorSummary>("coordinator_summary"),
  addPatient: (researchId: string, modelId: string) =>
    invoke<string>("coordinator_add_patient", { researchId, modelId }),
  removePatient: (patientId: string) =>
    invoke<void>("coordinator_remove_patient", { patientId }),
  addDocument: (patientId: string, documentType: string, filename: string, textContent: string) =>
    invoke<void>("coordinator_add_document", { patientId, documentType, filename, textContent }),
  importLlm: (patientId: string, rawJson: string) =>
    invoke<number>("coordinator_import_llm", { patientId, rawJson }),
  buildPackage: (
    reviewerId: string,
    displayName: string,
    patientIds: string[],
    password: string,
    destination: string
  ) =>
    invoke<PackageReceipt>("coordinator_build_package", {
      reviewerId,
      displayName,
      patientIds,
      password,
      destination,
    }),
  importResponse: (atbrPath: string) =>
    invoke<ImportSummary>("coordinator_import_response", { atbrPath }),
  listResults: () => invoke<ResultRow[]>("coordinator_list_results"),
};
