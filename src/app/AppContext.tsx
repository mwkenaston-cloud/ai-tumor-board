// Phase 1 in-memory application state. In Phase 2 the mutating actions here are
// re-pointed at the typed IPC layer (services/ipc.ts) backed by the SQLCipher
// repository; the component tree does not change.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import type {
  Assignment,
  NoteBlock,
  Patient,
  PatientState,
} from "../models/types";
import { seedAssignment, seedPatients } from "./seed";
import {
  aiBlock,
  reindex,
  similarityPercent,
  userBlock,
} from "../services/noteBlocks";
import { ipc, isTauri } from "../services/ipc";

export type Screen =
  | "home"
  | "unlock"
  | "coordinator"
  | "lobby"
  | "review"
  | "patientSurvey"
  | "completion"
  | "submitted";
export type SaveState = "idle" | "saving" | "saved";

/** Per-patient survey responses keyed by patientId, plus the final general survey. */
export interface SurveyData {
  perPatient: Record<string, Record<string, string>>;
  general: Record<string, string>;
}

interface AppState {
  role: "coordinator" | "reviewer" | null;
  screen: Screen;
  assignment: Assignment | null;
  patients: Record<string, Patient>;
  currentPatientId: string | null;
  saveState: SaveState;
  surveyData: SurveyData;
  reviewSeconds: number;
}

interface AppActions {
  enterReviewer: () => void;
  enterCoordinator: () => void;
  goHome: () => void;
  unlockAssignment: (path: string, password: string) => Promise<void>;
  useDemoAssignment: () => Promise<void>;
  openPatient: (patientId: string) => void;
  backToLobby: () => void;
  setNoteBlocks: (patientId: string, blocks: NoteBlock[]) => void;
  appendUserBlock: (patientId: string) => void;
  insertRecommendation: (patientId: string, recommendationId: string) => void;
  dismissRecommendation: (
    patientId: string,
    recommendationId: string,
    reason?: string
  ) => void;
  undoInsert: (patientId: string, recommendationId: string) => void;
  undoDismiss: (patientId: string, recommendationId: string) => void;
  removeBlock: (patientId: string, blockId: string) => void;
  completePatient: (patientId: string) => void;
  submitPatientSurvey: (patientId: string, answers: Record<string, string>) => void;
  startCompletion: () => void;
  submitAssignment: (generalAnswers: Record<string, string>) => void;
}

type AppContextValue = AppState & {
  actions: AppActions;
  currentPatient: Patient | null;
};

const AppContext = createContext<AppContextValue | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
  const [role, setRole] = useState<AppState["role"]>(null);
  const [screen, setScreen] = useState<Screen>("home");
  const [assignment, setAssignment] = useState<Assignment | null>(null);
  const [patients, setPatients] = useState<Record<string, Patient>>({});
  const [currentPatientId, setCurrentPatientId] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [surveyData, setSurveyData] = useState<SurveyData>({
    perPatient: {},
    general: {},
  });
  const [reviewSeconds, setReviewSeconds] = useState(0);

  const saveTimer = useRef<number | null>(null);
  const clockRef = useRef<number | null>(null);

  // Live review timer: ticks only while a patient is open on the review screen.
  useEffect(() => {
    if (screen !== "review") {
      if (clockRef.current) window.clearInterval(clockRef.current);
      return;
    }
    clockRef.current = window.setInterval(() => setReviewSeconds((s) => s + 1), 1000);
    return () => {
      if (clockRef.current) window.clearInterval(clockRef.current);
    };
  }, [screen]);

  // Always-current view of patients for use inside stable callbacks/persistence
  // (which would otherwise close over a stale snapshot).
  const patientsRef = useRef(patients);
  useEffect(() => {
    patientsRef.current = patients;
  }, [patients]);

  const persistDecision = useCallback(
    (
      patientId: string,
      recommendationId: string,
      patch: Partial<Patient["decisions"][number]>
    ) => {
      if (!isTauri()) return;
      const p = patientsRef.current[patientId];
      const existing = p?.decisions.find((d) => d.recommendationId === recommendationId);
      const decision = {
        recommendationId,
        status: "pending" as const,
        originalText: null,
        finalText: null,
        editDistance: null,
        similarityPercent: null,
        decisionElapsedSeconds: null,
        dismissalReason: null,
        decidedAt: null,
        ...existing,
        ...patch,
      };
      ipc.saveDecision(decision).catch((e) => console.error("save_decision failed", e));
    },
    []
  );

  // In non-Tauri (browser preview) mode there is no backend, so simulate a
  // save. Under Tauri, the autosave effect below drives the indicator instead.
  const flagSaved = useCallback(() => {
    if (isTauri()) return;
    setSaveState("saving");
    if (saveTimer.current) window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => setSaveState("saved"), 400);
  }, []);

  // Autosave the current patient's note blocks to the encrypted DB, debounced.
  // Fires on any change to the current patient (blocks, decisions), which is a
  // harmless re-save of the authoritative block array.
  useEffect(() => {
    if (!isTauri() || !currentPatientId) return;
    const p = patients[currentPatientId];
    if (!p) return;
    setSaveState("saving");
    const t = window.setTimeout(() => {
      ipc
        .saveNoteBlocks(p.id, p.noteBlocks)
        .then(() => setSaveState("saved"))
        .catch((e) => {
          console.error("autosave failed", e);
          setSaveState("idle");
        });
    }, 500);
    return () => window.clearTimeout(t);
  }, [patients, currentPatientId]);

  const updatePatient = useCallback(
    (patientId: string, fn: (p: Patient) => Patient) => {
      setPatients((prev) => {
        const p = prev[patientId];
        if (!p) return prev;
        return { ...prev, [patientId]: fn(p) };
      });
      flagSaved();
    },
    [flagSaved]
  );

  const setPatientStatus = useCallback(
    (patientId: string, status: PatientState) => {
      setAssignment((prev) =>
        prev
          ? {
              ...prev,
              patients: prev.patients.map((ps) =>
                ps.id === patientId ? { ...ps, status } : ps
              ),
            }
          : prev
      );
    },
    []
  );

  const enterReviewer = useCallback(() => {
    setRole("reviewer");
    if (isTauri()) {
      // Real flow: prompt for the assignment file + password on the unlock screen.
      setScreen("unlock");
    } else {
      // Browser preview has no backend — use the synthetic seed.
      setAssignment(seedAssignment());
      setPatients(seedPatients());
      setScreen("lobby");
    }
  }, []);

  const enterCoordinator = useCallback(() => {
    setRole("coordinator");
    setScreen("coordinator");
  }, []);

  const goHome = useCallback(() => {
    setRole(null);
    setAssignment(null);
    setPatients({});
    setCurrentPatientId(null);
    setScreen("home");
  }, []);

  const unlockAssignment = useCallback(async (path: string, password: string) => {
    const a = await ipc.openAssignment(path, password);
    setRole("reviewer");
    setAssignment(a);
    setPatients({});
    setScreen("lobby");
  }, []);

  const useDemoAssignment = useCallback(async () => {
    const a = await ipc.openDevAssignment();
    setRole("reviewer");
    setAssignment(a);
    setPatients({});
    setScreen("lobby");
  }, []);

  const openPatient = useCallback(
    (patientId: string) => {
      if (isTauri()) {
        ipc
          .openPatient(patientId)
          .then((p) => {
            const patient =
              p.noteBlocks.length > 0 ? p : { ...p, noteBlocks: reindex([userBlock("")]) };
            setPatients((prev) => ({ ...prev, [patientId]: patient }));
            setCurrentPatientId(patientId);
            setReviewSeconds(patient.elapsedSeconds);
            setScreen("review");
            setPatientStatus(patientId, patient.status as PatientState);
          })
          .catch((e) => console.error("open_patient failed", e));
        return;
      }
      setCurrentPatientId(patientId);
      setReviewSeconds(patientsRef.current[patientId]?.elapsedSeconds ?? 0);
      setScreen("review");
      updatePatient(patientId, (p) => {
        if (p.status === "complete") {
          return { ...p, status: "reopened" };
        }
        if (p.status === "not_started") {
          return {
            ...p,
            status: "in_progress",
            startedAt: p.startedAt ?? new Date().toISOString(),
            noteBlocks:
              p.noteBlocks.length > 0 ? p.noteBlocks : reindex([userBlock("")]),
          };
        }
        return p;
      });
      setPatientStatus(patientId, "in_progress");
    },
    [updatePatient, setPatientStatus]
  );

  const backToLobby = useCallback(() => {
    // Persist accumulated review time so it survives leaving/re-entering the
    // queue, and mirror it locally so a re-open resumes from where we left off.
    if (currentPatientId) {
      const pid = currentPatientId;
      if (isTauri()) {
        ipc.saveElapsed(pid, reviewSeconds).catch((e) => console.error("save_elapsed failed", e));
      }
      setPatients((prev) =>
        prev[pid] ? { ...prev, [pid]: { ...prev[pid], elapsedSeconds: reviewSeconds } } : prev
      );
    }
    setScreen("lobby");
    setCurrentPatientId(null);
  }, [currentPatientId, reviewSeconds]);

  const setNoteBlocks = useCallback(
    (patientId: string, blocks: NoteBlock[]) => {
      updatePatient(patientId, (p) => ({ ...p, noteBlocks: reindex(blocks) }));
    },
    [updatePatient]
  );

  const appendUserBlock = useCallback(
    (patientId: string) => {
      updatePatient(patientId, (p) => ({
        ...p,
        noteBlocks: reindex([...p.noteBlocks, userBlock("")]),
      }));
    },
    [updatePatient]
  );

  const insertRecommendation = useCallback(
    (patientId: string, recommendationId: string) => {
      updatePatient(patientId, (p) => {
        const rec = p.recommendations.find((r) => r.id === recommendationId);
        if (!rec) return p;
        const blocks = [...p.noteBlocks, aiBlock(rec.id, rec.text), userBlock("")];
        const decisions = upsertDecision(p, recommendationId, {
          status: "used",
          originalText: rec.text,
          finalText: rec.text,
          editDistance: 0,
          similarityPercent: 100,
          decidedAt: new Date().toISOString(),
        });
        return { ...p, noteBlocks: reindex(blocks), decisions };
      });
      const rec = patientsRef.current[patientId]?.recommendations.find(
        (r) => r.id === recommendationId
      );
      if (rec) {
        persistDecision(patientId, recommendationId, {
          status: "used",
          originalText: rec.text,
          finalText: rec.text,
          editDistance: 0,
          similarityPercent: 100,
          decidedAt: new Date().toISOString(),
        });
      }
    },
    [updatePatient, persistDecision]
  );

  const dismissRecommendation = useCallback(
    (patientId: string, recommendationId: string, reason?: string) => {
      updatePatient(patientId, (p) => {
        const decisions = upsertDecision(p, recommendationId, {
          status: "dismissed",
          dismissalReason: reason ?? null,
          decidedAt: new Date().toISOString(),
        });
        return { ...p, decisions };
      });
      persistDecision(patientId, recommendationId, {
        status: "dismissed",
        dismissalReason: reason ?? null,
        decidedAt: new Date().toISOString(),
      });
    },
    [updatePatient, persistDecision]
  );

  // Undo an insertion: remove the AI block(s) for this rec and reset to pending.
  const undoInsert = useCallback(
    (patientId: string, recommendationId: string) => {
      updatePatient(patientId, (p) => ({
        ...p,
        noteBlocks: reindex(
          p.noteBlocks.filter(
            (b) => !(b.type === "ai" && b.recommendationId === recommendationId)
          )
        ),
        decisions: p.decisions.filter((d) => d.recommendationId !== recommendationId),
      }));
      persistDecision(patientId, recommendationId, {
        status: "pending",
        finalText: null,
        decidedAt: null,
      });
    },
    [updatePatient, persistDecision]
  );

  // Undo a dismissal: reset the decision back to pending.
  const undoDismiss = useCallback(
    (patientId: string, recommendationId: string) => {
      updatePatient(patientId, (p) => ({
        ...p,
        decisions: p.decisions.filter((d) => d.recommendationId !== recommendationId),
      }));
      persistDecision(patientId, recommendationId, {
        status: "pending",
        dismissalReason: null,
        decidedAt: null,
      });
    },
    [updatePatient, persistDecision]
  );

  const removeBlock = useCallback(
    (patientId: string, blockId: string) => {
      const removed = patientsRef.current[patientId]?.noteBlocks.find((b) => b.id === blockId);
      updatePatient(patientId, (p) => {
        let decisions = p.decisions;
        if (removed && removed.type === "ai") {
          decisions = p.decisions.filter(
            (d) => d.recommendationId !== removed.recommendationId
          );
        }
        return {
          ...p,
          noteBlocks: reindex(p.noteBlocks.filter((b) => b.id !== blockId)),
          decisions,
        };
      });
      // Reset the decision to pending in the DB so the rec is no longer "used".
      if (removed && removed.type === "ai") {
        persistDecision(patientId, removed.recommendationId, {
          status: "pending",
          finalText: null,
          decidedAt: null,
        });
      }
    },
    [updatePatient, persistDecision]
  );

  const completePatient = useCallback(
    (patientId: string) => {
      updatePatient(patientId, (p) => ({
        ...p,
        status: "complete",
        completedAt: new Date().toISOString(),
      }));
      setPatientStatus(patientId, "complete");
      if (isTauri()) {
        ipc
          .completePatient(patientId, reviewSeconds)
          .then(() => ipc.loadAssignment())
          .then((a) => setAssignment(a))
          .catch((e) => console.error("complete_patient failed", e));
      }
      // Route to the per-patient survey when enabled, else straight to the queue.
      if (assignment?.settings.perPatientSurvey) {
        setScreen("patientSurvey");
      } else {
        backToLobby();
      }
    },
    [updatePatient, setPatientStatus, backToLobby, assignment, reviewSeconds]
  );

  const submitPatientSurvey = useCallback(
    (patientId: string, answers: Record<string, string>) => {
      setSurveyData((prev) => ({
        ...prev,
        perPatient: { ...prev.perPatient, [patientId]: answers },
      }));
      if (isTauri()) {
        ipc.saveSurvey(patientId, answers).catch((e) => console.error("save_survey failed", e));
      }
      flagSaved();
      backToLobby();
    },
    [flagSaved, backToLobby]
  );

  const startCompletion = useCallback(() => {
    if (assignment?.settings.generalSurvey) {
      setScreen("completion");
    } else {
      setScreen("submitted");
    }
  }, [assignment]);

  const submitAssignment = useCallback(
    (generalAnswers: Record<string, string>) => {
      setSurveyData((prev) => ({ ...prev, general: generalAnswers }));
      setAssignment((prev) => (prev ? { ...prev, state: "submitted" } : prev));
      if (isTauri()) {
        ipc
          .submitAssignment(generalAnswers)
          .catch((e) => console.error("submit_assignment failed", e));
      }
      flagSaved();
      setScreen("submitted");
    },
    [flagSaved]
  );

  const actions: AppActions = useMemo(
    () => ({
      enterReviewer,
      enterCoordinator,
      goHome,
      unlockAssignment,
      useDemoAssignment,
      openPatient,
      backToLobby,
      setNoteBlocks,
      appendUserBlock,
      insertRecommendation,
      dismissRecommendation,
      undoInsert,
      undoDismiss,
      removeBlock,
      completePatient,
      submitPatientSurvey,
      startCompletion,
      submitAssignment,
    }),
    [
      enterReviewer,
      enterCoordinator,
      goHome,
      unlockAssignment,
      useDemoAssignment,
      openPatient,
      backToLobby,
      setNoteBlocks,
      appendUserBlock,
      insertRecommendation,
      dismissRecommendation,
      undoInsert,
      undoDismiss,
      removeBlock,
      completePatient,
      submitPatientSurvey,
      startCompletion,
      submitAssignment,
    ]
  );

  const currentPatient = currentPatientId ? patients[currentPatientId] ?? null : null;

  const value: AppContextValue = {
    role,
    screen,
    assignment,
    patients,
    currentPatientId,
    saveState,
    surveyData,
    reviewSeconds,
    actions,
    currentPatient,
  };

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

// Recompute similarity when an AI block's decision is refreshed on edit.
export function recomputeAiSimilarity(original: string, edited: string) {
  return similarityPercent(original, edited);
}

function upsertDecision(
  p: Patient,
  recommendationId: string,
  patch: Partial<Patient["decisions"][number]>
): Patient["decisions"] {
  const existing = p.decisions.find((d) => d.recommendationId === recommendationId);
  const base = existing ?? {
    recommendationId,
    status: "pending" as const,
    originalText: null,
    finalText: null,
    editDistance: null,
    similarityPercent: null,
    decisionElapsedSeconds: null,
    dismissalReason: null,
    decidedAt: null,
  };
  const merged = { ...base, ...patch };
  const rest = p.decisions.filter((d) => d.recommendationId !== recommendationId);
  return [...rest, merged];
}

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used within AppProvider");
  return ctx;
}
