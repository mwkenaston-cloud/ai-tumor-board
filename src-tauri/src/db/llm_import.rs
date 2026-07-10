//! Validate coordinator-supplied LLM output against the bundled JSON Schema and
//! normalize the multi-phase format (phase3..phase6, merged by
//! recommendation_id, as in the prototype) into recommendation rows.
#![allow(dead_code)]

use rusqlite::{params, Connection};
use serde_json::Value;

use super::connection::DbError;
use super::models::Recommendation;
use super::repository::now_iso;

const SCHEMA: &str = include_str!("../../schemas/llm-output.schema.json");

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("schema validation failed: {0}")]
    Invalid(String),
    #[error("duplicate recommendation_id: {0}")]
    Duplicate(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

pub struct NormalizedImport {
    pub model: String,
    pub prompt_version: String,
    pub recommendations: Vec<Recommendation>,
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

/// Parse, validate against the schema, and normalize. Rejects malformed JSON,
/// schema violations, and duplicate recommendation ids.
pub fn validate_and_normalize(patient_id: &str, raw_json: &str) -> Result<NormalizedImport, LlmError> {
    let instance: Value = serde_json::from_str(raw_json).map_err(|e| LlmError::Json(e.to_string()))?;

    let schema: Value = serde_json::from_str(SCHEMA).expect("bundled schema parses");
    let validator = jsonschema::validator_for(&schema)
        .map_err(|e| LlmError::Invalid(e.to_string()))?;
    if !validator.is_valid(&instance) {
        let msg = validator
            .iter_errors(&instance)
            .next()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "does not match schema".into());
        return Err(LlmError::Invalid(msg));
    }

    let meta = &instance["session_metadata"];
    let model = str_field(meta, "model").unwrap_or_default();
    let prompt_version = str_field(meta, "prompt_version").unwrap_or_default();

    // Index the auxiliary phases by recommendation_id.
    let empty = Vec::new();
    let by_id = |arr: &str| -> std::collections::HashMap<String, Value> {
        instance
            .get(arr)
            .and_then(|v| v.as_array())
            .unwrap_or(&empty)
            .iter()
            .filter_map(|item| str_field(item, "recommendation_id").map(|id| (id, item.clone())))
            .collect()
    };
    let phase4 = by_id("phase4_safety_assessment");
    let phase5 = by_id("phase5_condensed_recommendations");
    let priority: std::collections::HashMap<String, i64> = instance
        .get("phase6_synthesis")
        .and_then(|s| s.get("priority_ranking"))
        .and_then(|v| v.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter_map(|item| {
            let id = str_field(item, "recommendation_id")?;
            let rank = item.get("rank").and_then(|r| r.as_i64())?;
            Some((id, rank))
        })
        .collect();

    let phase3 = instance["phase3_recommendations"].as_array().cloned().unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let mut recommendations = Vec::new();

    for (pos, rec) in phase3.iter().enumerate() {
        let rec_id = str_field(rec, "recommendation_id").unwrap_or_default();
        if !seen.insert(rec_id.clone()) {
            return Err(LlmError::Duplicate(rec_id));
        }

        let full_text = str_field(rec, "recommendation_text").unwrap_or_default();
        let condensed = phase5.get(&rec_id).and_then(|p| str_field(p, "condensed_note"));
        let text = condensed.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| full_text.clone());

        let safety = phase4.get(&rec_id);
        let safety_score = safety
            .and_then(|s| s.get("safety_score_final_pct"))
            .and_then(|x| x.as_f64());

        let metadata = serde_json::json!({
            "clinical_rationale": rec.get("clinical_rationale"),
            "safety_score_rationale": safety.and_then(|s| s.get("safety_score_rationale")),
            "drug_interactions": safety.and_then(|s| s.get("drug_interactions")),
            "adverse_event_profile": safety.and_then(|s| s.get("adverse_event_profile")),
            "monitoring_plan": safety.and_then(|s| s.get("monitoring_plan")),
        });

        recommendations.push(Recommendation {
            id: format!("{patient_id}:{rec_id}"),
            patient_id: patient_id.to_string(),
            position: pos as i64,
            priority_rank: priority.get(&rec_id).copied(),
            temperature_level: rec.get("temperature_level").and_then(|x| x.as_i64()),
            temperature_label: str_field(rec, "temperature_label"),
            evidence_tier: str_field(rec, "evidence_tier"),
            risk_score: rec.get("risk_score").and_then(|x| x.as_f64()),
            safety_score,
            title: Some(rec_id.clone()),
            text,
            full_text: Some(full_text),
            rationale: str_field(rec, "clinical_rationale"),
            metadata: Some(metadata),
            is_custom: false,
        });
    }

    Ok(NormalizedImport { model, prompt_version, recommendations })
}

/// Persist a normalized import: record the raw run for provenance and replace the
/// patient's recommendation rows.
pub fn store_import(
    conn: &Connection,
    patient_id: &str,
    raw_json: &str,
    normalized: &NormalizedImport,
) -> Result<(), DbError> {
    let run_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO llm_runs(llm_run_id, patient_id, model_name, prompt_version, raw_json, imported_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![run_id, patient_id, normalized.model, normalized.prompt_version, raw_json, now_iso()],
    )?;
    conn.execute("DELETE FROM recommendations WHERE patient_id = ?1", [patient_id])?;
    for rec in &normalized.recommendations {
        let meta = rec.metadata.as_ref().map(|m| m.to_string());
        conn.execute(
            "INSERT INTO recommendations(recommendation_id, patient_id, llm_run_id, position, priority_rank,
                 temperature_level, temperature_label, evidence_tier, risk_score, safety_score, title,
                 recommendation_text, full_text, rationale, metadata_json, is_custom)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,0)",
            params![
                rec.id, patient_id, run_id, rec.position, rec.priority_rank, rec.temperature_level,
                rec.temperature_label, rec.evidence_tier, rec.risk_score, rec.safety_score, rec.title,
                rec.text, rec.full_text, rec.rationale, meta
            ],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
      "session_metadata": { "model": "gpt-x", "prompt_version": "v3" },
      "phase3_recommendations": [
        { "recommendation_id": "R1", "recommendation_text": "Full text one", "temperature_level": 2, "evidence_tier": "I", "risk_score": 2 },
        { "recommendation_id": "R2", "recommendation_text": "Full text two", "temperature_level": 4 }
      ],
      "phase4_safety_assessment": [ { "recommendation_id": "R1", "safety_score_final_pct": 88 } ],
      "phase5_condensed_recommendations": [ { "recommendation_id": "R1", "condensed_note": "Condensed one" } ],
      "phase6_synthesis": { "priority_ranking": [ { "recommendation_id": "R1", "rank": 1 } ] }
    }"#;

    #[test]
    fn normalizes_and_merges_phases() {
        let n = validate_and_normalize("PT-1", GOOD).unwrap();
        assert_eq!(n.model, "gpt-x");
        assert_eq!(n.recommendations.len(), 2);
        let r1 = &n.recommendations[0];
        assert_eq!(r1.id, "PT-1:R1");
        assert_eq!(r1.text, "Condensed one"); // phase5 preferred
        assert_eq!(r1.full_text.as_deref(), Some("Full text one"));
        assert_eq!(r1.safety_score, Some(88.0));
        assert_eq!(r1.priority_rank, Some(1));
    }

    #[test]
    fn rejects_missing_required_fields() {
        let bad = r#"{ "session_metadata": { "model": "m" }, "phase3_recommendations": [] }"#;
        assert!(matches!(validate_and_normalize("PT-1", bad), Err(LlmError::Invalid(_))));
    }

    #[test]
    fn rejects_bad_json() {
        assert!(matches!(validate_and_normalize("PT-1", "{not json"), Err(LlmError::Json(_))));
    }

    #[test]
    fn rejects_duplicate_recommendation_ids() {
        let dup = r#"{
          "session_metadata": { "model": "m", "prompt_version": "v" },
          "phase3_recommendations": [
            { "recommendation_id": "R1", "recommendation_text": "a" },
            { "recommendation_id": "R1", "recommendation_text": "b" }
          ]
        }"#;
        assert!(matches!(validate_and_normalize("PT-1", dup), Err(LlmError::Duplicate(_))));
    }

    #[test]
    fn out_of_range_score_is_rejected() {
        let bad = r#"{
          "session_metadata": { "model": "m", "prompt_version": "v" },
          "phase3_recommendations": [ { "recommendation_id": "R1", "recommendation_text": "a", "risk_score": 99 } ]
        }"#;
        assert!(matches!(validate_and_normalize("PT-1", bad), Err(LlmError::Invalid(_))));
    }
}
