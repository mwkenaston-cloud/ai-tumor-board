//! End-to-end coordinator → reviewer → response → import loop, exercising the
//! library functions the commands compose. Verifies the coordinator public key
//! flows from workspace to package to response seal, and that import unseals and
//! merges correctly.

use rusqlite::params;

use super::models::{NoteBlock, RecommendationDecision};
use super::{llm_import, packaging, repository as repo, response};
use crate::crypto::container::{self, ContainerRole};
use crate::crypto::response_seal;

fn tempdir(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!("atb-e2e-{tag}-{nanos}"));
    std::fs::create_dir_all(&p).unwrap();
    p
}

const LLM_JSON: &str = r#"{
  "session_metadata": { "model": "gpt-x", "prompt_version": "v3" },
  "phase3_recommendations": [
    { "recommendation_id": "R1", "recommendation_text": "Adjuvant chemotherapy per guideline.", "temperature_level": 2, "evidence_tier": "I", "risk_score": 2 }
  ],
  "phase4_safety_assessment": [ { "recommendation_id": "R1", "safety_score_final_pct": 88 } ],
  "phase5_condensed_recommendations": [ { "recommendation_id": "R1", "condensed_note": "Start adjuvant chemo." } ],
  "phase6_synthesis": { "priority_ranking": [ { "recommendation_id": "R1", "rank": 1 } ] }
}"#;

#[test]
fn full_coordinator_to_results_loop() {
    let dir = tempdir("loop");

    // ── Coordinator workspace ──────────────────────────────────────────────
    let ws_path = dir.join("workspace.atb");
    let workspace = container::create(&ws_path, "coord-pw", ContainerRole::Coordinator).unwrap();
    workspace
        .execute(
            "INSERT INTO studies(study_id,title,protocol_version,schema_version,created_at)
             VALUES ('STUDY-1','Study','v1',1,'2026-07-10T00:00:00Z')",
            [],
        )
        .unwrap();
    let (coord_secret, coord_public) = response_seal::generate_keypair();
    repo::set_metadata(&workspace, "coordinator_secret_hex", &hex(&coord_secret)).unwrap();
    repo::set_metadata(&workspace, "coordinator_public_hex", &hex(&coord_public)).unwrap();

    // Add a patient + document + validated LLM import.
    workspace
        .execute(
            "INSERT INTO patients(patient_id,study_id,research_id,display_label,clinical_question,position,status,elapsed_seconds)
             VALUES ('PT-9','STUDY-1','TUM-9','Case','What therapy?',0,'not_started',0)",
            [],
        )
        .unwrap();
    workspace
        .execute(
            "INSERT INTO source_documents(document_id,patient_id,document_type,filename,mime_type,text_content,byte_size,sha256,created_at)
             VALUES ('d1','PT-9','notes','n.txt','text/plain','clinical note',13,'sha',?1)",
            [repo::now_iso()],
        )
        .unwrap();
    let normalized = llm_import::validate_and_normalize("PT-9", LLM_JSON).unwrap();
    llm_import::store_import(&workspace, "PT-9", LLM_JSON, &normalized).unwrap();

    // ── Register the reviewer + assignment (as the command does) ───────────
    workspace
        .execute(
            "INSERT INTO reviewers(reviewer_id,display_name,role,assignment_status) VALUES ('REV-9','Reviewer 9','reviewer','ready')",
            [],
        )
        .unwrap();
    workspace
        .execute(
            "INSERT INTO reviewer_assignments(reviewer_id,patient_id,position) VALUES ('REV-9','PT-9',0)",
            [],
        )
        .unwrap();

    // ── Build reviewer package (embeds assignment id + coordinator pubkey) ──
    let pkg_path = dir.join("reviewer.atb");
    let receipt = packaging::build_package(
        &workspace,
        &pkg_path,
        "reviewer-pw",
        "REV-9",
        &["PT-9".to_string()],
        &coord_public,
    )
    .unwrap();
    assert_eq!(receipt.patient_count, 1);

    // ── Reviewer opens, does work ──────────────────────────────────────────
    let mut pkg = container::open(&pkg_path, "reviewer-pw").unwrap();
    let p = repo::load_patient(&pkg, "REV-9", "PT-9").unwrap();
    assert_eq!(p.recommendations.len(), 1);
    let rec_id = p.recommendations[0].id.clone();
    let blocks = vec![
        NoteBlock { id: "b1".into(), block_type: "user".into(), recommendation_id: None, original_text: None, current_text: "Agree with plan.".into(), position: 0 },
        NoteBlock { id: "b2".into(), block_type: "ai".into(), recommendation_id: Some(rec_id.clone()), original_text: Some("Start adjuvant chemo.".into()), current_text: "Start adjuvant chemo.".into(), position: 1 },
    ];
    repo::save_note_blocks(&mut pkg, "REV-9", "PT-9", &blocks).unwrap();
    repo::upsert_decision(&pkg, "REV-9", &RecommendationDecision {
        recommendation_id: rec_id, status: "used".into(), original_text: Some("Start adjuvant chemo.".into()),
        final_text: Some("Start adjuvant chemo.".into()), edit_distance: Some(0), similarity_percent: Some(100.0),
        decision_elapsed_seconds: Some(20), dismissal_reason: None, decided_at: Some(repo::now_iso()),
    }).unwrap();

    // ── Reviewer builds sealed response using metadata embedded in the package ─
    let assignment_id = repo::get_metadata(&pkg, "assignment_id").unwrap().unwrap();
    let pub_hex = repo::get_metadata(&pkg, "coordinator_public_hex").unwrap().unwrap();
    let pubkey = hex32(&pub_hex);
    let atbr = dir.join("response.atbr");
    let rresp = response::build_response(&pkg, &pkg_path, &atbr, &assignment_id, &pubkey).unwrap();
    assert_eq!(rresp.reviewer_id, "REV-9");

    // ── Coordinator imports + merges ───────────────────────────────────────
    let imported = response::import_response(&atbr, &coord_secret).unwrap();
    assert_eq!(imported.header.assignment_id, assignment_id);
    let pt = &imported.patients[0];
    assert!(pt.final_text.contains("Agree with plan."));
    assert!(pt.attribution.pct_derived_from_llm > 0);

    response::merge_into_results(&workspace, &imported).unwrap();
    // Duplicate rejected.
    assert!(response::merge_into_results(&workspace, &imported).is_err());

    // Results recorded.
    let n: i64 = workspace
        .query_row("SELECT count(*) FROM imported_responses WHERE reviewer_id='REV-9'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

// Confirms a response sealed for one coordinator cannot be opened by another.
#[test]
fn response_cannot_be_opened_by_other_coordinator() {
    let dir = tempdir("wrongcoord");
    let ws_path = dir.join("ws.atb");
    let ws = container::create(&ws_path, "pw", ContainerRole::Coordinator).unwrap();
    ws.execute("INSERT INTO studies(study_id,title,protocol_version,schema_version,created_at) VALUES ('STUDY-1','S','v1',1,'t')", []).unwrap();
    ws.execute("INSERT INTO patients(patient_id,study_id,research_id,display_label,clinical_question,position,status,elapsed_seconds) VALUES ('PT-1','STUDY-1','R','L','Q',0,'not_started',0)", []).unwrap();
    ws.execute("INSERT INTO source_documents(document_id,patient_id,document_type,text_content,byte_size,sha256,created_at) VALUES ('d','PT-1','notes','x',1,'s',?1)", params![repo::now_iso()]).unwrap();
    let norm = llm_import::validate_and_normalize("PT-1", LLM_JSON).unwrap();
    llm_import::store_import(&ws, "PT-1", LLM_JSON, &norm).unwrap();

    ws.execute("INSERT INTO reviewers(reviewer_id,display_name,role,assignment_status) VALUES ('REV-1','R1','reviewer','ready')", []).unwrap();
    ws.execute("INSERT INTO reviewer_assignments(reviewer_id,patient_id,position) VALUES ('REV-1','PT-1',0)", []).unwrap();

    let (_good_secret, good_public) = response_seal::generate_keypair();
    let (attacker_secret, _attacker_public) = response_seal::generate_keypair();

    let pkg_path = dir.join("r.atb");
    packaging::build_package(&ws, &pkg_path, "rp", "REV-1", &["PT-1".to_string()], &good_public).unwrap();
    let pkg = container::open(&pkg_path, "rp").unwrap();
    let asg = repo::get_metadata(&pkg, "assignment_id").unwrap().unwrap();
    let atbr = dir.join("r.atbr");
    response::build_response(&pkg, &pkg_path, &atbr, &asg, &good_public).unwrap();

    // The wrong coordinator secret cannot import.
    assert!(response::import_response(&atbr, &attacker_secret).is_err());
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for b in bytes {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

fn hex32(s: &str) -> [u8; 32] {
    let v: Vec<u8> = (0..64).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect();
    v.try_into().unwrap()
}
