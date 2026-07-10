import { useApp } from "../app/AppContext";

export default function SaveIndicator() {
  const { saveState } = useApp();
  const label =
    saveState === "saving" ? "Saving…" : saveState === "saved" ? "All changes saved" : "";
  if (!label) return null;
  return (
    <span className={`save-indicator ${saveState}`}>
      <span className="save-dot" />
      {label}
    </span>
  );
}
