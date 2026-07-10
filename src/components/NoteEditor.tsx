import { useEffect, useRef, useState } from "react";
import type { NoteBlock } from "../models/types";
import { similarityPercent } from "../services/noteBlocks";

/**
 * A single editable block. Content is written to the DOM exactly once (on mount
 * / when the block id changes) and thereafter managed by the browser, so React
 * re-renders of the parent never reset the caret. Text flows back up on input.
 */
function Block({
  block,
  readOnly,
  onChange,
  onRemove,
}: {
  block: NoteBlock;
  readOnly: boolean;
  onChange: (id: string, text: string) => void;
  onRemove: (id: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [edited, setEdited] = useState(
    block.type === "ai" ? block.currentText.trim() !== block.originalText.trim() : false
  );
  const [sim, setSim] = useState(
    block.type === "ai" ? similarityPercent(block.originalText, block.currentText) : 100
  );

  // Seed DOM content once per block identity.
  useEffect(() => {
    if (ref.current && ref.current.textContent !== block.currentText) {
      ref.current.textContent = block.currentText;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [block.id]);

  const handleInput = () => {
    const text = ref.current?.textContent ?? "";
    onChange(block.id, text);
    if (block.type === "ai") {
      const isEdited = text.trim() !== block.originalText.trim();
      setEdited(isEdited);
      setSim(similarityPercent(block.originalText, text));
    }
  };

  if (block.type === "user") {
    return (
      <div
        ref={ref}
        className="nb-user"
        contentEditable={!readOnly}
        suppressContentEditableWarning
        data-placeholder="Write your assessment and plan…"
        onInput={handleInput}
      />
    );
  }

  return (
    <div className={`nb-ai-wrapper ${edited ? "edited" : ""}`}>
      <div className="nb-ai-badge">
        <span>AI · {block.recommendationId}</span>
        <span className={`nb-ai-status ${edited ? "edited" : "original"}`}>
          {edited ? "Edited" : "AI original"}
        </span>
        {!readOnly && (
          <button
            className="nb-remove-btn"
            title="Remove this recommendation from the note"
            onClick={() => onRemove(block.id)}
          >
            ✕
          </button>
        )}
      </div>
      <div
        ref={ref}
        className="nb-ai"
        contentEditable={!readOnly}
        suppressContentEditableWarning
        onInput={handleInput}
      />
      {edited && <div className="nb-ai-diff">Edited — {sim}% similar to AI original</div>}
    </div>
  );
}

interface Props {
  blocks: NoteBlock[];
  readOnly?: boolean;
  onChangeBlock: (id: string, text: string) => void;
  onRemoveBlock: (id: string) => void;
  onAddParagraph: () => void;
}

export default function NoteEditor({
  blocks,
  readOnly = false,
  onChangeBlock,
  onRemoveBlock,
  onAddParagraph,
}: Props) {
  return (
    <div className="note-editor">
      {blocks.map((b) => (
        <Block
          key={b.id}
          block={b}
          readOnly={readOnly}
          onChange={onChangeBlock}
          onRemove={onRemoveBlock}
        />
      ))}
      {!readOnly && (
        <button
          className="btn btn-ghost btn-sm"
          style={{ marginTop: 10 }}
          onClick={onAddParagraph}
        >
          + Add paragraph
        </button>
      )}
    </div>
  );
}
