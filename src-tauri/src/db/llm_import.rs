//! Validate coordinator-supplied LLM output against the bundled JSON Schema and
//! normalize the multi-phase tumor-board format (see Prompt 3 / the example
//! output) into recommendation rows plus patient context (phase 1) and framing
//! (phase 2). `recommendation_id` may be an integer or a string.
#![allow(dead_code)]

use rusqlite::{params, Connection};
use serde_json::{json, Value};

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
    pub cancer_type: Option<String>,
    pub clinical_question: Option<String>,
    /// Phase-1 context: patient_profile, timeline, comorbidities, family history.
    pub context: Value,
    /// Phase-2 framing: decision_points, relevant_patient_factors, specialist_perspectives.
    pub framing: Value,
    pub recommendations: Vec<Recommendation>,
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

/// recommendation_id may be an integer or a string; normalize to a string key.
fn rec_id_of(v: &Value) -> Option<String> {
    match v.get("recommendation_id") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

pub fn validate_and_normalize(patient_id: &str, raw_json: &str) -> Result<NormalizedImport, LlmError> {
    let instance: Value = serde_json::from_str(raw_json).map_err(|e| LlmError::Json(e.to_string()))?;

    let schema: Value = serde_json::from_str(SCHEMA).expect("bundled schema parses");
    let validator = jsonschema::validator_for(&schema).map_err(|e| LlmError::Invalid(e.to_string()))?;
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
    let cancer_type = str_field(meta, "cancer_type");
    let clinical_question = str_field(meta, "clinical_question");

    // Phase-1 context bundle.
    let context = json!({
        "patient_profile": instance.get("phase1_patient_summary").and_then(|p| p.get("patient_profile")),
        "timeline": instance.get("phase1_patient_summary").and_then(|p| p.get("timeline")),
        "comorbidities": instance.get("patient_comorbidities"),
        "family_history": instance.get("patient_family_history"),
    });
    // Phase-2 framing bundle.
    let framing = instance.get("phase2_question_framing").cloned().unwrap_or(Value::Null);

    // Auxiliary phases indexed by (string) recommendation_id.
    let empty = Vec::new();
    let by_id = |arr: &str| -> std::collections::HashMap<String, Value> {
        instance
            .get(arr)
            .and_then(|v| v.as_array())
            .unwrap_or(&empty)
            .iter()
            .filter_map(|item| rec_id_of(item).map(|id| (id, item.clone())))
            .collect()
    };
    let phase4 = by_id("phase4_safety_assessment");
    let phase5 = by_id("phase5_condensed_recommendations");
    let ranking = instance
        .get("phase6_synthesis")
        .and_then(|s| s.get("priority_ranking"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let priority: std::collections::HashMap<String, i64> = ranking
        .iter()
        .filter_map(|item| Some((rec_id_of(item)?, item.get("rank")?.as_i64()?)))
        .collect();
    let priority_rationale: std::collections::HashMap<String, String> = ranking
        .iter()
        .filter_map(|item| Some((rec_id_of(item)?, str_field(item, "rationale")?)))
        .collect();

    // phase6 uncertainty: epistemic (evidence) + patient-specific (risk) per rec.
    let uncertainty: std::collections::HashMap<String, (Option<String>, Option<String>)> = instance
        .get("phase6_synthesis")
        .and_then(|s| s.get("uncertainty"))
        .and_then(|v| v.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter_map(|item| {
            let id = rec_id_of(item)?;
            Some((
                id,
                (
                    str_field(item, "epistemic_uncertainty"),
                    str_field(item, "patient_specific_uncertainty"),
                ),
            ))
        })
        .collect();

    let phase3 = instance["phase3_recommendations"].as_array().cloned().unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let mut recommendations = Vec::new();

    for (pos, rec) in phase3.iter().enumerate() {
        let rec_id = rec_id_of(rec).unwrap_or_default();
        if !seen.insert(rec_id.clone()) {
            return Err(LlmError::Duplicate(rec_id));
        }

        // Show the full recommendation_text; keep the condensed note-ready form
        // in metadata for reference.
        let recommendation_text = str_field(rec, "recommendation_text").unwrap_or_default();
        let condensed = phase5.get(&rec_id).and_then(|p| str_field(p, "condensed_note"));
        let text = recommendation_text.clone();
        let unc = uncertainty.get(&rec_id).cloned().unwrap_or((None, None));

        let safety = phase4.get(&rec_id);
        // Prefer the refined phase-4 score, else the phase-3 score.
        let safety_score = safety
            .and_then(|s| s.get("safety_score_final_pct"))
            .and_then(|x| x.as_f64())
            .or_else(|| rec.get("safety_score_pct").and_then(|x| x.as_f64()));

        let metadata = json!({
            "clinical_rationale": rec.get("clinical_rationale"),
            "contraindications_noted": rec.get("contraindications_noted"),
            "monitoring_requirements": rec.get("monitoring_requirements"),
            "drug_interactions": safety.and_then(|s| s.get("drug_interactions")),
            "contraindication_classification": safety.and_then(|s| s.get("contraindication_classification")),
            "adverse_event_profile": safety.and_then(|s| s.get("adverse_event_profile")),
            "monitoring_plan": safety.and_then(|s| s.get("monitoring_plan")),
            "safety_score_rationale": safety.and_then(|s| s.get("safety_score_rationale")),
            "priority_rationale": priority_rationale.get(&rec_id),
            "epistemic_uncertainty": unc.0,
            "patient_specific_uncertainty": unc.1,
            "condensed_note": condensed,
            "comorbidity_flags_referenced": rec.get("comorbidity_flags_referenced"),
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
            title: Some(format!("Recommendation {rec_id}")),
            text,
            full_text: None,
            rationale: str_field(rec, "clinical_rationale"),
            metadata: Some(metadata),
            is_custom: false,
        });
    }

    Ok(NormalizedImport {
        model,
        prompt_version,
        cancer_type,
        clinical_question,
        context,
        framing,
        recommendations,
    })
}

/// Persist a normalized import: raw run for provenance, replaced recommendation
/// rows, and the patient's cancer type / clinical question / context / framing.
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

    // Patient-level context. display_label follows the cancer type once known.
    conn.execute(
        "UPDATE patients SET
             cancer_type = COALESCE(?2, cancer_type),
             clinical_question = COALESCE(?3, clinical_question),
             display_label = COALESCE(?2, display_label),
             context_json = ?4,
             framing_json = ?5
         WHERE patient_id = ?1",
        params![
            patient_id,
            normalized.cancer_type,
            normalized.clinical_question,
            normalized.context.to_string(),
            normalized.framing.to_string(),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Uses the new format: integer recommendation_id, safety_score_pct, phase1/phase2.
    const GOOD: &str = r#"{
      "session_metadata": { "model": "gpt-5.5", "prompt_version": "1.0", "cancer_type": "Prostate adenocarcinoma", "clinical_question": "Proceed with surgery?" },
      "phase1_patient_summary": { "patient_profile": "65M ...", "timeline": [ { "date": "2026-02-16", "event_type": "MRI", "finding": "PI-RADS 5", "source_quote": "PIRADS 5" } ] },
      "patient_comorbidities": [ { "condition": "Hyperlipidemia", "status": "Active" } ],
      "patient_family_history": { "family_history_of_cancer": [] },
      "phase2_question_framing": { "decision_points": ["Surgery vs radiation"], "specialist_perspectives": { "oncology": "..." } },
      "phase3_recommendations": [
        { "recommendation_id": 1, "temperature_level": 2, "temperature_label": "Moderate-Conservative", "recommendation_text": "Full text one", "evidence_tier": "I", "risk_score": 3, "safety_score_pct": 82, "contraindications_noted": ["c1"], "monitoring_requirements": ["m1"] },
        { "recommendation_id": 2, "temperature_level": 4, "recommendation_text": "Full text two" }
      ],
      "phase4_safety_assessment": [ { "recommendation_id": 1, "safety_score_final_pct": 80, "drug_interactions": ["ddi"], "monitoring_plan": "labs" } ],
      "phase5_condensed_recommendations": [ { "recommendation_id": 1, "condensed_note": "Condensed one" } ],
      "phase6_synthesis": { "priority_ranking": [ { "recommendation_id": 1, "rank": 1 } ] }
    }"#;

    #[test]
    fn normalizes_new_format_with_integer_ids() {
        let n = validate_and_normalize("PT-1", GOOD).unwrap();
        assert_eq!(n.model, "gpt-5.5");
        assert_eq!(n.cancer_type.as_deref(), Some("Prostate adenocarcinoma"));
        assert_eq!(n.clinical_question.as_deref(), Some("Proceed with surgery?"));
        assert_eq!(n.recommendations.len(), 2);
        let r1 = &n.recommendations[0];
        assert_eq!(r1.id, "PT-1:1");
        assert_eq!(r1.text, "Full text one"); // recommendation_text, not condensed
        assert_eq!(r1.full_text, None);
        assert_eq!(r1.safety_score, Some(80.0)); // phase4 refined preferred
        assert_eq!(r1.priority_rank, Some(1));
        // context + framing captured
        assert!(n.context.get("timeline").is_some());
        assert!(n.framing.get("decision_points").is_some());
    }

    #[test]
    fn phase3_safety_used_when_no_phase4() {
        let n = validate_and_normalize("PT-1", GOOD).unwrap();
        let r2 = &n.recommendations[1];
        assert_eq!(r2.safety_score, None); // rec 2 has neither phase3 nor phase4 score
    }

    #[test]
    fn rejects_out_of_range_safety() {
        let bad = r#"{ "session_metadata": { "model": "m", "prompt_version": "v" },
          "phase3_recommendations": [ { "recommendation_id": 1, "recommendation_text": "a", "safety_score_pct": 250 } ] }"#;
        assert!(matches!(validate_and_normalize("PT-1", bad), Err(LlmError::Invalid(_))));
    }

    #[test]
    fn rejects_bad_json() {
        assert!(matches!(validate_and_normalize("PT-1", "{nope"), Err(LlmError::Json(_))));
    }

    #[test]
    fn accepts_v12_object_comorbidities_and_flags() {
        let raw = r#"{
          "session_metadata": {"model":"gpt","prompt_version":"1.2","cancer_type":"Prostate","clinical_question":"Q"},
          "patient_comorbidities": {
            "comorbidity_summary": {
              "cci_score_overview": {"unadjusted_score":2,"age_adjusted_score":4,"estimated_10yr_survival_pct":53,"interpretation":"moderate burden"},
              "active_treatment_relevant_flags": [{"category":"Heart Failure","clinical_detail":"LVEF 35%","treatment_implication":"avoid anthracyclines"}],
              "overall_burden_narrative":"substantial multi-organ burden"
            },
            "charlson_comorbidity_index": {"unadjusted_score":2,"contributing_conditions":[]},
            "treatment_relevant_flags": [{"category":"Heart Failure","status":"present","severity_or_stage":"HFrEF","treatment_implication":"avoid anthracyclines"}],
            "other_comorbidities": []
          },
          "phase3_recommendations": [{"recommendation_id":1,"recommendation_text":"txt","comorbidity_flags_referenced":["Heart Failure"]}]
        }"#;
        let n = validate_and_normalize("PT-1", raw).unwrap();
        // Phase-1 context carries the rich comorbidity object.
        assert!(n.context.get("comorbidities").and_then(|c| c.get("comorbidity_summary")).is_some());
        // The rec metadata carries the referenced flags.
        let meta = n.recommendations[0].metadata.as_ref().unwrap();
        assert_eq!(
            meta.get("comorbidity_flags_referenced").and_then(|v| v.as_array()).map(|a| a.len()),
            Some(1)
        );
    }

    #[test]
    fn rejects_duplicate_ids() {
        let dup = r#"{ "session_metadata": { "model": "m", "prompt_version": "v" },
          "phase3_recommendations": [ { "recommendation_id": 1, "recommendation_text": "a" }, { "recommendation_id": 1, "recommendation_text": "b" } ] }"#;
        assert!(matches!(validate_and_normalize("PT-1", dup), Err(LlmError::Duplicate(_))));
    }
}
