// Note-block logic ported from the prototype's block editor. Kept framework-free
// and pure so it can be unit-tested and reused by the AI-attribution export.

import type { NoteBlock } from "../models/types";

let _counter = 0;
/** Stable, collision-resistant block id (never rely on DOM order for identity). */
export function newBlockId(): string {
  _counter += 1;
  return `b${Date.now().toString(36)}${_counter.toString(36)}`;
}

export function userBlock(text = ""): NoteBlock {
  return { id: newBlockId(), type: "user", currentText: text, position: 0 };
}

export function aiBlock(recommendationId: string, text: string): NoteBlock {
  return {
    id: newBlockId(),
    type: "ai",
    recommendationId,
    originalText: text,
    currentText: text,
    position: 0,
  };
}

/** Reassign sequential positions so the array order is the source of truth. */
export function reindex(blocks: NoteBlock[]): NoteBlock[] {
  return blocks.map((b, i) => ({ ...b, position: i }));
}

/** Levenshtein edit distance (character-level), as in the prototype. */
export function levenshtein(a: string, b: string): number {
  if (a === b) return 0;
  if (a.length === 0) return b.length;
  if (b.length === 0) return a.length;
  let prev = Array.from({ length: b.length + 1 }, (_, i) => i);
  let curr = new Array<number>(b.length + 1);
  for (let i = 1; i <= a.length; i++) {
    curr[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      curr[j] = Math.min(curr[j - 1] + 1, prev[j] + 1, prev[j - 1] + cost);
    }
    [prev, curr] = [curr, prev];
  }
  return prev[b.length];
}

/** Similarity 0–100 between original AI text and its edited form. */
export function similarityPercent(original: string, edited: string): number {
  if (!original && !edited) return 100;
  const dist = levenshtein(original, edited);
  const maxLen = Math.max(original.length, edited.length) || 1;
  return Math.round((1 - dist / maxLen) * 100);
}

/** True if an AI block has been changed from the LLM's original text. */
export function isAiEdited(block: Extract<NoteBlock, { type: "ai" }>): boolean {
  return block.currentText.trim() !== block.originalText.trim();
}

/**
 * Deterministically derive the final plain-text note from ordered blocks.
 * User blocks contribute their text; AI blocks contribute their current text.
 * Empty blocks are skipped; blocks are joined with a blank line, matching the
 * prototype's export behavior.
 */
export function deriveNoteText(blocks: NoteBlock[]): string {
  return blocks
    .map((b) => b.currentText.trim())
    .filter((t) => t.length > 0)
    .join("\n\n");
}

export interface AttributionMetrics {
  wordCount: number;
  charCount: number;
  charsFromLlmUnmodified: number;
  charsFromLlmEdited: number;
  charsTypedByPhysician: number;
  pctAiUnmodified: number;
  pctAiEdited: number;
  pctPhysicianOriginal: number;
  pctDerivedFromLlm: number;
}

/**
 * AI-attribution metrics over the note, mirroring the prototype's
 * buildOutputJson() breakdown. Character counts are computed from the trimmed
 * block texts that actually appear in the derived note.
 */
export function attributionMetrics(blocks: NoteBlock[]): AttributionMetrics {
  let unmodified = 0;
  let edited = 0;
  let physician = 0;

  for (const b of blocks) {
    const len = b.currentText.trim().length;
    if (len === 0) continue;
    if (b.type === "user") {
      physician += len;
    } else if (isAiEdited(b)) {
      edited += len;
    } else {
      unmodified += len;
    }
  }

  const total = unmodified + edited + physician || 1;
  const text = deriveNoteText(blocks);
  const words = text.split(/\s+/).filter(Boolean).length;
  const pct = (n: number) => Math.round((n / total) * 100);

  return {
    wordCount: words,
    charCount: text.length,
    charsFromLlmUnmodified: unmodified,
    charsFromLlmEdited: edited,
    charsTypedByPhysician: physician,
    pctAiUnmodified: pct(unmodified),
    pctAiEdited: pct(edited),
    pctPhysicianOriginal: pct(physician),
    pctDerivedFromLlm: pct(unmodified + edited),
  };
}
