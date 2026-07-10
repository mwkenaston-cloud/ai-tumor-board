// Phase 1 in-memory application state. In Phase 2 the mutating actions here are
// re-pointed at the typed IPC layer (services/ipc.ts) backed by the SQLCipher
// repository; the component tree does not change.

import {
  createContext,
  useCallback,
  useContext,
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

export type Screen =
  | "home"
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
}

interface AppActions {
  enterReviewer: () => void;
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

  const saveTimer = useRef<number | null>(null);

  // Simulated debounced "save" for the Phase 1 UI; replaced by real autosave
  // against the repository in Phase 2.
  const flagSaved = useCallback(() => {
    setSaveState("saving");
    if (saveTimer.current) window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => setSaveState("saved"), 400);
  }, []);

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
    setAssignment(seedAssignment());
    setPatients(seedPatients());
    setScreen("lobby");
  }, []);

  const openPatient = useCallback(
    (patientId: string) => {
      setCurrentPatientId(patientId);
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
    setScreen("lobby");
    setCurrentPatientId(null);
  }, []);

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
    },
    [updatePatient]
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
    },
    [updatePatient]
  );

  const removeBlock = useCallback(
    (patientId: string, blockId: string) => {
      updatePatient(patientId, (p) => {
        const removed = p.noteBlocks.find((b) => b.id === blockId);
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
    },
    [updatePatient]
  );

  const completePatient = useCallback(
    (patientId: string) => {
      updatePatient(patientId, (p) => ({
        ...p,
        status: "complete",
        completedAt: new Date().toISOString(),
      }));
      setPatientStatus(patientId, "complete");
      // Route to the per-patient survey when enabled, else straight to the queue.
      if (assignment?.settings.perPatientSurvey) {
        setScreen("patientSurvey");
      } else {
        backToLobby();
      }
    },
    [updatePatient, setPatientStatus, backToLobby, assignment]
  );

  const submitPatientSurvey = useCallback(
    (patientId: string, answers: Record<string, string>) => {
      setSurveyData((prev) => ({
        ...prev,
        perPatient: { ...prev.perPatient, [patientId]: answers },
      }));
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
      flagSaved();
      setScreen("submitted");
    },
    [flagSaved]
  );

  const actions: AppActions = useMemo(
    () => ({
      enterReviewer,
      openPatient,
      backToLobby,
      setNoteBlocks,
      appendUserBlock,
      insertRecommendation,
      dismissRecommendation,
      removeBlock,
      completePatient,
      submitPatientSurvey,
      startCompletion,
      submitAssignment,
    }),
    [
      enterReviewer,
      openPatient,
      backToLobby,
      setNoteBlocks,
      appendUserBlock,
      insertRecommendation,
      dismissRecommendation,
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
