//! Synthetic, non-PHI demo data used to bootstrap a dev/reviewer session and by
//! repository tests. In production, an assignment DB is instead populated by the
//! coordinator's package builder (Phase 3/4). Recommendation ids are made
//! globally unique (`PT:local`) to satisfy the single-column primary key.

use rusqlite::{params, Connection};

use super::connection::DbError;
use super::repository::now_iso;

pub fn seed_demo(conn: &Connection, reviewer_id: &str) -> Result<(), DbError> {
    let now = now_iso();

    let settings = serde_json::json!({
        "studyTitle": "AI Tumor Board Concordance Study",
        "showPriority": true, "showEvidence": true, "showSafety": true,
        "showTemperature": true, "showDetails": true, "allowDismiss": true,
        "showTimer": true, "perPatientSurvey": true, "generalSurvey": true
    })
    .to_string();

    conn.execute(
        "INSERT OR REPLACE INTO studies(study_id, title, protocol_version, schema_version,
             contact_email, instructions, settings_json, created_at)
         VALUES ('STUDY-1', 'AI Tumor Board Concordance Study', 'v1', 1,
             'coordinator@example.org', ?1, ?2, ?3)",
        params![
            "Review each assigned patient's source documents and the AI-generated recommendations. \
             Compose an independent assessment & plan, inserting or dismissing AI recommendations as \
             you see fit. Complete the short survey after each patient.",
            settings, now
        ],
    )?;

    conn.execute(
        "INSERT OR REPLACE INTO reviewers(reviewer_id, display_name, role, assignment_status)
         VALUES (?1, 'Reviewer 04', 'reviewer', 'ready')",
        params![reviewer_id],
    )?;

    seed_patient(
        conn, reviewer_id, 0, "PT-1", "TUM-0042",
        "Stage III colon adenocarcinoma",
        "72yo with resected stage III colon adenocarcinoma (pT3N1). What adjuvant systemic therapy is most appropriate?",
        &[
            ("notes", "oncology-note.txt",
             "HPI: 72yo s/p right hemicolectomy for a T3 ascending colon lesion. 1/18 nodes positive, \
              margins negative, ECOG 0.\n\nAssessment: pT3N1 (stage IIIB) colon adenocarcinoma, MSS, resected."),
            ("pathology", "pathology.txt",
             "Moderately differentiated adenocarcinoma invading subserosa (pT3). 1/18 nodes positive. \
              LVI present. MMR proficient (MSS). Margins negative."),
        ],
        &[
            RecSeed { local: "1", position: 0, priority: 1, temp: 2, temp_label: "Moderate-Conservative",
                tier: "I", risk: 2.0, safety: 88.0, title: "Adjuvant CAPOX x 3 months",
                text: "Recommend adjuvant CAPOX for 3 months given low-risk (T3N1) stage III MSS disease, per IDEA.",
                full: "For low-risk stage III colon cancer the IDEA collaboration supports 3 months of CAPOX as non-inferior to 6 months with less neurotoxicity." },
            RecSeed { local: "2", position: 1, priority: 3, temp: 4, temp_label: "Aggressive",
                tier: "II", risk: 4.0, safety: 62.0, title: "6 months FOLFOX",
                text: "Alternative: 6 months FOLFOX for maximal control, accepting higher cumulative neurotoxicity.",
                full: "6 months of oxaliplatin-based therapy remains an option; weigh against increased grade 3+ neuropathy." },
        ],
        &now,
    )?;

    seed_patient(
        conn, reviewer_id, 1, "PT-2", "TUM-0043",
        "EGFR+ lung adenocarcinoma",
        "64yo never-smoker with metastatic EGFR exon 19 del lung adenocarcinoma. First-line systemic therapy?",
        &[
            ("notes", "note.txt",
             "64yo never-smoker, metastatic lung adenocarcinoma with EGFR exon 19 deletion. Brain MRI: two small asymptomatic metastases. ECOG 1."),
        ],
        &[
            RecSeed { local: "1", position: 0, priority: 1, temp: 2, temp_label: "Moderate-Conservative",
                tier: "I", risk: 2.0, safety: 90.0, title: "First-line osimertinib",
                text: "Start osimertinib 80 mg daily; CNS-active and preferred for EGFR exon 19 del incl. asymptomatic brain mets.",
                full: "Osimertinib showed superior PFS/OS (FLAURA) with strong CNS activity, the preferred first-line TKI for EGFR exon 19 del / L858R." },
        ],
        &now,
    )?;

    Ok(())
}

struct RecSeed {
    local: &'static str,
    position: i64,
    priority: i64,
    temp: i64,
    temp_label: &'static str,
    tier: &'static str,
    risk: f64,
    safety: f64,
    title: &'static str,
    text: &'static str,
    full: &'static str,
}

#[allow(clippy::too_many_arguments)]
fn seed_patient(
    conn: &Connection,
    reviewer_id: &str,
    position: i64,
    patient_id: &str,
    research_id: &str,
    label: &str,
    clinical_question: &str,
    docs: &[(&str, &str, &str)],
    recs: &[RecSeed],
    now: &str,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR REPLACE INTO patients(patient_id, study_id, research_id, display_label,
             clinical_question, position, status, elapsed_seconds)
         VALUES (?1, 'STUDY-1', ?2, ?3, ?4, ?5, 'not_started', 0)",
        params![patient_id, research_id, label, clinical_question, position],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO reviewer_assignments(reviewer_id, patient_id, position)
         VALUES (?1, ?2, ?3)",
        params![reviewer_id, patient_id, position],
    )?;

    for (i, (dtype, fname, body)) in docs.iter().enumerate() {
        conn.execute(
            "INSERT OR REPLACE INTO source_documents(document_id, patient_id, document_type,
                 filename, mime_type, text_content, byte_size, sha256, created_at)
             VALUES (?1,?2,?3,?4,'text/plain',?5,?6,?7,?8)",
            params![
                format!("{patient_id}-doc-{i}"), patient_id, dtype, fname, body,
                body.len() as i64, format!("seed-{patient_id}-{i}"), now
            ],
        )?;
    }

    for rec in recs {
        conn.execute(
            "INSERT OR REPLACE INTO recommendations(recommendation_id, patient_id, position,
                 priority_rank, temperature_level, temperature_label, evidence_tier, risk_score,
                 safety_score, title, recommendation_text, full_text, is_custom)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,0)",
            params![
                format!("{patient_id}:{}", rec.local), patient_id, rec.position, rec.priority,
                rec.temp, rec.temp_label, rec.tier, rec.risk, rec.safety, rec.title, rec.text, rec.full
            ],
        )?;
    }

    Ok(())
}
