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
  cipherVersion: () => invoke<string | null>("cipher_version"),
};
