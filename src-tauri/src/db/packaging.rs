//! Build a reviewer-specific `.atb` assignment package from a coordinator
//! workspace: validate, copy only the selected reviewer's assigned patients
//! into a fresh encrypted database, and return an integrity receipt.
//!
//! Command-wired with coordinator mode; exercised now by unit tests.
#![allow(dead_code)]

use std::path::Path;

use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::connection::DbError;
use crate::crypto::container::{self, ContainerError, ContainerRole};

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Container(#[from] ContainerError),
    #[error("io error: {0}")]
    Io(String),
    #[error("patient {0} failed validation: {1}")]
    Validation(String, String),
}

impl From<std::io::Error> for PackageError {
    fn from(e: std::io::Error) -> Self {
        PackageError::Io(e.to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageReceipt {
    pub sha256: String,
    pub patient_count: usize,
    pub reviewer_id: String,
    pub assignment_id: String,
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

/// Validate a patient is complete enough to ship: research id, clinical
/// question, at least one source document, and at least one recommendation.
fn validate_patient(source: &Connection, patient_id: &str) -> Result<(), PackageError> {
    let (research_id, clinical_question): (Option<String>, Option<String>) = source
        .query_row(
            "SELECT research_id, clinical_question FROM patients WHERE patient_id = ?1",
            [patient_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(DbError::from)?;

    let fail = |m: &str| PackageError::Validation(patient_id.to_string(), m.to_string());

    if research_id.as_deref().unwrap_or("").trim().is_empty() {
        return Err(fail("missing research id"));
    }
    if clinical_question.as_deref().unwrap_or("").trim().is_empty() {
        return Err(fail("missing clinical question"));
    }
    let docs: i64 = source
        .query_row("SELECT count(*) FROM source_documents WHERE patient_id = ?1", [patient_id], |r| r.get(0))
        .map_err(DbError::from)?;
    if docs < 1 {
        return Err(fail("no source documents"));
    }
    let recs: i64 = source
        .query_row("SELECT count(*) FROM recommendations WHERE patient_id = ?1", [patient_id], |r| r.get(0))
        .map_err(DbError::from)?;
    if recs < 1 {
        return Err(fail("no recommendations / LLM output"));
    }
    Ok(())
}

/// Build the package. `dest_path` must not already exist. Returns a receipt with
/// the SHA-256 of the finished encrypted file.
pub fn build_package(
    source: &Connection,
    dest_path: &Path,
    dest_password: &str,
    reviewer_id: &str,
    patient_ids: &[String],
    coordinator_public: &[u8; 32],
) -> Result<PackageReceipt, PackageError> {
    if patient_ids.is_empty() {
        return Err(PackageError::Validation("<none>".into(), "no patients selected".into()));
    }
    for pid in patient_ids {
        validate_patient(source, pid)?;
    }

    let assignment_id = format!("ASG-{}", uuid::Uuid::new_v4());

    // Fresh encrypted destination, then copy the frozen slice into it.
    {
        let dest = container::create(dest_path, dest_password, ContainerRole::Reviewer)?;
        copy_study(source, &dest)?;
        copy_reviewer(source, &dest, reviewer_id)?;
        for (pos, pid) in patient_ids.iter().enumerate() {
            copy_patient(source, &dest, reviewer_id, pid, pos as i64)?;
        }
        // Embed response-routing metadata: assignment id + coordinator key.
        crate::db::repository::set_metadata(&dest, "assignment_id", &assignment_id)?;
        crate::db::repository::set_metadata(&dest, "coordinator_public_hex", &hex_encode(coordinator_public))?;
        // Reopen-and-validate happens below on a clean handle.
    }

    // Reopen and integrity-check the finished file before trusting it.
    let verify = container::open(dest_path, dest_password)?;
    let count: i64 = verify
        .query_row("SELECT count(*) FROM reviewer_assignments WHERE reviewer_id = ?1", [reviewer_id], |r| r.get(0))
        .map_err(DbError::from)?;
    drop(verify);

    let sha256 = sha256_file(dest_path)?;
    Ok(PackageReceipt {
        sha256,
        patient_count: count as usize,
        reviewer_id: reviewer_id.to_string(),
        assignment_id,
    })
}

fn copy_study(source: &Connection, dest: &Connection) -> Result<(), PackageError> {
    let row = source.query_row(
        "SELECT study_id, title, protocol_version, schema_version, contact_email, instructions, settings_json, created_at
         FROM studies LIMIT 1",
        [],
        |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?,
            r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, Option<String>>(6)?, r.get::<_, String>(7)?,
        )),
    ).map_err(DbError::from)?;
    dest.execute(
        "INSERT OR REPLACE INTO studies(study_id, title, protocol_version, schema_version, contact_email, instructions, settings_json, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7],
    ).map_err(DbError::from)?;
    Ok(())
}

fn copy_reviewer(source: &Connection, dest: &Connection, reviewer_id: &str) -> Result<(), PackageError> {
    let (name, specialty, role): (Option<String>, Option<String>, Option<String>) = source
        .query_row(
            "SELECT display_name, specialty, role FROM reviewers WHERE reviewer_id = ?1",
            [reviewer_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(DbError::from)?;
    dest.execute(
        "INSERT OR REPLACE INTO reviewers(reviewer_id, display_name, specialty, role, assignment_status)
         VALUES (?1,?2,?3,?4,'ready')",
        params![reviewer_id, name, specialty, role.unwrap_or_else(|| "reviewer".into())],
    )
    .map_err(DbError::from)?;
    Ok(())
}

fn copy_patient(
    source: &Connection,
    dest: &Connection,
    reviewer_id: &str,
    patient_id: &str,
    position: i64,
) -> Result<(), PackageError> {
    let p = source.query_row(
        "SELECT study_id, research_id, model_id, display_label, clinical_question, cancer_type, context_json, framing_json FROM patients WHERE patient_id = ?1",
        [patient_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, Option<String>>(6)?, r.get::<_, Option<String>>(7)?)),
    ).map_err(DbError::from)?;
    dest.execute(
        "INSERT OR REPLACE INTO patients(patient_id, study_id, research_id, model_id, display_label, clinical_question, cancer_type, context_json, framing_json, position, status, elapsed_seconds)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'not_started',0)",
        params![patient_id, p.0, p.1, p.2, p.3, p.4, p.5, p.6, p.7, position],
    ).map_err(DbError::from)?;
    dest.execute(
        "INSERT OR REPLACE INTO reviewer_assignments(reviewer_id, patient_id, position) VALUES (?1,?2,?3)",
        params![reviewer_id, patient_id, position],
    ).map_err(DbError::from)?;

    // Documents
    let mut ds = source.prepare(
        "SELECT document_id, document_type, filename, mime_type, text_content, binary_content, byte_size, sha256, created_at
         FROM source_documents WHERE patient_id = ?1",
    ).map_err(DbError::from)?;
    let docs = ds.query_map([patient_id], |r| {
        Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?, r.get::<_, Option<Vec<u8>>>(5)?, r.get::<_, Option<i64>>(6)?, r.get::<_, String>(7)?, r.get::<_, String>(8)?,
        ))
    }).map_err(DbError::from)?;
    for d in docs {
        let d = d.map_err(DbError::from)?;
        dest.execute(
            "INSERT OR REPLACE INTO source_documents(document_id, patient_id, document_type, filename, mime_type, text_content, binary_content, byte_size, sha256, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![d.0, patient_id, d.1, d.2, d.3, d.4, d.5, d.6, d.7, d.8],
        ).map_err(DbError::from)?;
    }

    // LLM runs (optional)
    let mut ls = source.prepare(
        "SELECT llm_run_id, model_name, prompt_version, raw_json, imported_at FROM llm_runs WHERE patient_id = ?1",
    ).map_err(DbError::from)?;
    let runs = ls.query_map([patient_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?))
    }).map_err(DbError::from)?;
    for run in runs {
        let run = run.map_err(DbError::from)?;
        dest.execute(
            "INSERT OR REPLACE INTO llm_runs(llm_run_id, patient_id, model_name, prompt_version, raw_json, imported_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![run.0, patient_id, run.1, run.2, run.3, run.4],
        ).map_err(DbError::from)?;
    }

    // Recommendations
    let mut rs = source.prepare(
        "SELECT recommendation_id, llm_run_id, position, priority_rank, temperature_level, temperature_label,
                evidence_tier, risk_score, safety_score, title, recommendation_text, full_text, rationale, metadata_json, is_custom
         FROM recommendations WHERE patient_id = ?1",
    ).map_err(DbError::from)?;
    let recs = rs.query_map([patient_id], |r| {
        Ok((
            r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<i64>>(3)?,
            r.get::<_, Option<i64>>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, Option<String>>(6)?, r.get::<_, Option<f64>>(7)?,
            r.get::<_, Option<f64>>(8)?, r.get::<_, Option<String>>(9)?, r.get::<_, String>(10)?, r.get::<_, Option<String>>(11)?,
            r.get::<_, Option<String>>(12)?, r.get::<_, Option<String>>(13)?, r.get::<_, i64>(14)?,
        ))
    }).map_err(DbError::from)?;
    for rec in recs {
        let rec = rec.map_err(DbError::from)?;
        dest.execute(
            "INSERT OR REPLACE INTO recommendations(recommendation_id, patient_id, llm_run_id, position, priority_rank,
                 temperature_level, temperature_label, evidence_tier, risk_score, safety_score, title, recommendation_text,
                 full_text, rationale, metadata_json, is_custom)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![rec.0, patient_id, rec.1, rec.2, rec.3, rec.4, rec.5, rec.6, rec.7, rec.8, rec.9, rec.10, rec.11, rec.12, rec.13, rec.14],
        ).map_err(DbError::from)?;
    }

    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, PackageError> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut s = String::with_capacity(64);
    use std::fmt::Write;
    for b in digest {
        let _ = write!(s, "{:02x}", b);
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{repository as repo, seed};

    fn tempfile(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        p.push(format!("atb-pkg-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p.join(name)
    }

    #[test]
    fn builds_reviewer_specific_package() {
        let src_path = tempfile("workspace.atb");
        let source = container::create(&src_path, "coordinator-pw", ContainerRole::Coordinator).unwrap();
        seed::seed_demo(&source, "REV-A").unwrap();

        // Package only PT-1 for the reviewer, with a distinct reviewer password.
        let (_sec, pubkey) = crate::crypto::response_seal::generate_keypair();
        let dest_path = tempfile("reviewer.atb");
        let receipt = build_package(&source, &dest_path, "reviewer-pw", "REV-A", &["PT-1".to_string()], &pubkey).unwrap();
        assert_eq!(receipt.patient_count, 1);
        assert_eq!(receipt.sha256.len(), 64);
        assert!(receipt.assignment_id.starts_with("ASG-"));

        // The package opens with the reviewer password and contains only PT-1.
        let dest = container::open(&dest_path, "reviewer-pw").unwrap();
        let asg = repo::load_assignment(&dest, "REV-A").unwrap();
        assert_eq!(asg.patients.len(), 1);
        assert_eq!(asg.patients[0].id, "PT-1");
        let p = repo::load_patient(&dest, "REV-A", "PT-1").unwrap();
        assert!(!p.recommendations.is_empty());
        assert!(!p.documents.is_empty());
    }

    #[test]
    fn rejects_patient_missing_data() {
        let src_path = tempfile("workspace2.atb");
        let source = container::create(&src_path, "pw", ContainerRole::Coordinator).unwrap();
        seed::seed_demo(&source, "REV-A").unwrap();
        // Blank out PT-1's clinical question to trip validation.
        source.execute("UPDATE patients SET clinical_question='' WHERE patient_id='PT-1'", []).unwrap();

        let (_sec, pubkey) = crate::crypto::response_seal::generate_keypair();
        let dest_path = tempfile("reviewer2.atb");
        let err = build_package(&source, &dest_path, "pw2", "REV-A", &["PT-1".to_string()], &pubkey).unwrap_err();
        assert!(matches!(err, PackageError::Validation(_, _)), "got {err:?}");
    }
}
