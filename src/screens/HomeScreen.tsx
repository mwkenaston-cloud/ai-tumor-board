import { useApp } from "../app/AppContext";

export default function HomeScreen() {
  const { actions } = useApp();
  return (
    <div className="center-screen">
      <div style={{ textAlign: "center" }}>
        <h2 style={{ marginBottom: 6 }}>AI Tumor Board</h2>
        <p style={{ color: "var(--muted)", marginBottom: 28 }}>
          Offline reviewer for AI tumor board research assignments
        </p>
        <div className="role-grid">
          <div className="role-card" onClick={actions.enterReviewer}>
            <div className="role-icon">🩺</div>
            <h3>Physician</h3>
            <p>Open an assignment, review assigned patients, and record your assessment.</p>
          </div>
          <div className="role-card" onClick={actions.enterCoordinator}>
            <div className="role-icon">🗂️</div>
            <h3>Coordinator</h3>
            <p>Build studies, import AI output, create assignment packages, and import responses.</p>
          </div>
        </div>
        <p style={{ fontSize: 12, color: "var(--muted-2)", marginTop: 24 }}>
          Phase 1 preview — synthetic sample data, in-memory only.
        </p>
      </div>
    </div>
  );
}
