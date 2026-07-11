//! Parse a single combined clinical-source `.txt` into per-type documents.
//!
//! Sections are introduced by a header line — "Txt Imaging", "Txt Clinical
//! Notes", "Txt Pathology", "Txt Labs" (case-insensitive) — and individual
//! entries within a section are separated by a line of "=" characters. The
//! whole section (dividers preserved) becomes one source document of that type.
#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSection {
    pub document_type: String,
    pub content: String,
}

/// Map a candidate header line to a document type, if it is one.
fn header_type(line: &str) -> Option<&'static str> {
    match line.trim().to_ascii_lowercase().as_str() {
        "txt imaging" => Some("imaging"),
        "txt clinical notes" => Some("notes"),
        "txt pathology" => Some("pathology"),
        "txt labs" => Some("labs"),
        _ => None,
    }
}

/// Split the combined text into sections. Content before the first header is
/// ignored; empty sections are dropped.
pub fn parse_combined(text: &str) -> Vec<ParsedSection> {
    let mut sections: Vec<ParsedSection> = Vec::new();
    let mut current: Option<(String, Vec<&str>)> = None;

    for line in text.lines() {
        if let Some(t) = header_type(line) {
            if let Some((doc_type, lines)) = current.take() {
                push_section(&mut sections, doc_type, lines);
            }
            current = Some((t.to_string(), Vec::new()));
        } else if let Some((_, lines)) = current.as_mut() {
            lines.push(line);
        }
    }
    if let Some((doc_type, lines)) = current.take() {
        push_section(&mut sections, doc_type, lines);
    }
    sections
}

fn push_section(out: &mut Vec<ParsedSection>, doc_type: String, lines: Vec<&str>) {
    let content = lines.join("\n").trim().to_string();
    if !content.is_empty() {
        out.push(ParsedSection { document_type: doc_type, content });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Txt Clinical Notes
Visit 1 note.
================================================================================
Visit 2 note.
Txt Pathology
Path report body.
Txt Imaging
MRI report.
Txt Labs
PSA 31.4
";

    #[test]
    fn splits_into_four_types() {
        let s = parse_combined(SAMPLE);
        let types: Vec<_> = s.iter().map(|x| x.document_type.as_str()).collect();
        assert_eq!(types, ["notes", "pathology", "imaging", "labs"]);
    }

    #[test]
    fn keeps_multiple_entries_and_dividers_in_one_section() {
        let s = parse_combined(SAMPLE);
        let notes = &s[0];
        assert!(notes.content.contains("Visit 1 note."));
        assert!(notes.content.contains("Visit 2 note."));
        assert!(notes.content.contains("===="));
    }

    #[test]
    fn header_matching_is_case_insensitive() {
        let s = parse_combined("txt LABS\nvalue\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].document_type, "labs");
    }

    #[test]
    fn preamble_before_first_header_is_ignored() {
        let s = parse_combined("junk preamble\nTxt Labs\nX\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].content, "X");
    }

    #[test]
    fn empty_sections_dropped() {
        let s = parse_combined("Txt Imaging\n\nTxt Labs\nY\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].document_type, "labs");
    }
}
