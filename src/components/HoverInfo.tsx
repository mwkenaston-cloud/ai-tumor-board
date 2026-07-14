import { useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

/**
 * Wraps an inline element and shows a styled floating tooltip on hover — richer
 * than a native title attribute (full text, no truncation, dark card), mirroring
 * the prototype's score tooltips. Portaled to <body> so panel overflow never
 * clips it.
 */
export default function HoverInfo({
  tip,
  title,
  className,
  children,
}: {
  tip: string;
  title?: string;
  className?: string;
  children: ReactNode;
}) {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);

  const show = (e: React.MouseEvent) => {
    const r = e.currentTarget.getBoundingClientRect();
    const width = 320;
    const x = Math.min(r.left, window.innerWidth - width - 12);
    setPos({ x: Math.max(12, x), y: r.bottom + 6 });
  };

  return (
    <span
      className={className}
      style={{ cursor: "help" }}
      onMouseEnter={show}
      onMouseLeave={() => setPos(null)}
    >
      {children}
      {pos &&
        tip &&
        createPortal(
          <div
            style={{
              position: "fixed",
              left: pos.x,
              top: pos.y,
              width: 320,
              background: "#0f172a",
              color: "#e2e8f0",
              padding: "10px 14px",
              borderRadius: 8,
              fontSize: 12,
              fontWeight: 400,
              lineHeight: 1.6,
              letterSpacing: 0,
              textTransform: "none",
              boxShadow: "0 8px 24px rgba(0,0,0,0.45)",
              zIndex: 9999,
              pointerEvents: "none",
              whiteSpace: "normal",
            }}
          >
            {title && (
              <div style={{ fontWeight: 700, color: "#7dd3fc", fontSize: 11, textTransform: "uppercase", letterSpacing: 0.5, marginBottom: 4 }}>
                {title}
              </div>
            )}
            {tip}
          </div>,
          document.body
        )}
    </span>
  );
}
