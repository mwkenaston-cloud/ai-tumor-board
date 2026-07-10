import "./styles/app.css";
import { AppProvider, useApp } from "./app/AppContext";
import HomeScreen from "./screens/HomeScreen";
import UnlockScreen from "./screens/UnlockScreen";
import CoordinatorScreen from "./screens/CoordinatorScreen";
import PhysicianLobby from "./screens/PhysicianLobby";
import PatientReview from "./screens/PatientReview";
import PatientSurvey from "./screens/PatientSurvey";
import CompletionSurvey from "./screens/CompletionSurvey";
import SubmissionScreen from "./screens/SubmissionScreen";
import SaveIndicator from "./components/SaveIndicator";

function Header() {
  const { role, screen, assignment } = useApp();
  return (
    <header className="header">
      <div className="header-left">
        <h1>AI Tumor Board</h1>
        {role && (
          <span className={`mode-pill ${role}`}>
            {role === "reviewer" ? "Physician" : "Coordinator"}
          </span>
        )}
        {assignment && (
          <span style={{ fontSize: 12, color: "var(--muted)" }}>
            {assignment.studyTitle} · {assignment.reviewerDisplayName}
          </span>
        )}
      </div>
      <div className="header-right">{screen === "review" && <SaveIndicator />}</div>
    </header>
  );
}

function Shell() {
  const { screen } = useApp();
  return (
    <div className="app">
      <Header />
      <div style={{ position: "relative", flex: 1, display: "flex", overflow: "hidden" }}>
        {screen === "home" && <HomeScreen />}
        {screen === "unlock" && <UnlockScreen />}
        {screen === "coordinator" && <CoordinatorScreen />}
        {screen === "lobby" && <PhysicianLobby />}
        {screen === "review" && <PatientReview />}
        {screen === "patientSurvey" && <PatientSurvey />}
        {screen === "completion" && <CompletionSurvey />}
        {screen === "submitted" && <SubmissionScreen />}
      </div>
    </div>
  );
}

export default function App() {
  return (
    <AppProvider>
      <Shell />
    </AppProvider>
  );
}
