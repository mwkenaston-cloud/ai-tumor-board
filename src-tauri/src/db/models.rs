//! Serde DTOs mirroring src/models/types.ts. `camelCase` on the wire so the
//! React layer consumes them directly; these are the payloads the typed IPC
//! commands return and accept.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDocument {
    pub id: String,
    pub patient_id: String,
    pub document_type: String,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub text_content: Option<String>,
    pub byte_size: Option<i64>,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub id: String,
    pub patient_id: String,
    pub position: i64,
    pub priority_rank: Option<i64>,
    pub temperature_level: Option<i64>,
    pub temperature_label: Option<String>,
    pub evidence_tier: Option<String>,
    pub risk_score: Option<f64>,
    pub safety_score: Option<f64>,
    pub title: Option<String>,
    pub text: String,
    pub full_text: Option<String>,
    pub rationale: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub is_custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationDecision {
    pub recommendation_id: String,
    pub status: String,
    pub original_text: Option<String>,
    pub final_text: Option<String>,
    pub edit_distance: Option<i64>,
    pub similarity_percent: Option<f64>,
    pub decision_elapsed_seconds: Option<i64>,
    pub dismissal_reason: Option<String>,
    pub decided_at: Option<String>,
}

/// Flat form of the TS `NoteBlock` union (user vs ai). Optional fields are
/// absent for user blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteBlock {
    pub id: String,
    #[serde(rename = "type")]
    pub block_type: String, // "user" | "ai"
    #[serde(default)]
    pub recommendation_id: Option<String>,
    #[serde(default)]
    pub original_text: Option<String>,
    pub current_text: String,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patient {
    pub id: String,
    pub research_id: Option<String>,
    pub display_label: String,
    pub clinical_question: Option<String>,
    pub position: i64,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub elapsed_seconds: i64,
    pub documents: Vec<SourceDocument>,
    pub recommendations: Vec<Recommendation>,
    pub decisions: Vec<RecommendationDecision>,
    pub note_blocks: Vec<NoteBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientSummary {
    pub id: String,
    pub research_id: Option<String>,
    pub display_label: String,
    pub position: i64,
    pub status: String,
    pub elapsed_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub study_id: String,
    pub study_title: String,
    pub protocol_version: String,
    pub schema_version: i64,
    pub contact_email: Option<String>,
    pub instructions: Option<String>,
    pub settings: serde_json::Value,
    pub reviewer_id: String,
    pub reviewer_display_name: Option<String>,
    pub state: String,
    pub patients: Vec<PatientSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub event_type: String,
    #[serde(default)]
    pub patient_id: Option<String>,
    pub event_time: String,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}
