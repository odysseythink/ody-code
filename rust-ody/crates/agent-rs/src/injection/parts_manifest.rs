/// A single row in the parts manifest table.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestPart {
    pub file: String,
    pub scope: String,
}

/// Parsed parts manifest — mirrors TS `PartsManifest`.
#[derive(Debug, Clone, PartialEq)]
pub struct PartsManifest {
    pub all_done: bool,
    pub next: Option<ManifestPart>,
}

/// Parse a markdown table with columns `# | File | Scope | Status` to find the first pending row.
/// Mirrors TS `parsePartsManifest`.
pub fn parse_parts_manifest(content: &str) -> Option<PartsManifest> {
    let rows: Vec<(&str, &str, &str)> = content
        .lines()
        .filter(|line| line.starts_with('|') && !line.starts_with("|---") && !line.contains("File"))
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            // cells[0] is empty (leading |), cells[1] is #, cells[2] is File, cells[3] is Scope, cells[4] is Status
            if cells.len() >= 5 {
                let file = cells.get(2).unwrap_or(&"");
                let scope = cells.get(3).unwrap_or(&"");
                let status = cells.get(4).unwrap_or(&"");
                if !file.is_empty() {
                    Some((*file, *scope, *status))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if rows.is_empty() {
        return None;
    }

    let all_done = !rows
        .iter()
        .any(|(_, _, status)| status.to_lowercase() == "pending");

    let next = if all_done {
        None
    } else {
        rows.iter()
            .find(|(_, _, status)| status.to_lowercase() == "pending")
            .map(|(file, scope, _)| ManifestPart {
                file: file.to_string(),
                scope: scope.to_string(),
            })
    };

    Some(PartsManifest { all_done, next })
}

/// Extract all file names from a parts manifest table.
/// Mirrors TS `parseManifestFiles`.
pub fn parse_manifest_files(content: &str) -> Vec<String> {
    content
        .lines()
        .filter(|line| line.starts_with('|') && !line.starts_with("|---") && !line.contains("File"))
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            let file = cells.get(2).unwrap_or(&"");
            if file.is_empty() {
                None
            } else {
                Some(file.to_string())
            }
        })
        .collect()
}

/// Count done/pending rows — mirrors TS `countManifestRows`.
pub fn count_manifest_rows(content: &str) -> Option<(usize, usize)> {
    let mut done = 0usize;
    let mut pending = 0usize;
    let mut found_table = false;
    for line in content.lines() {
        if line.starts_with('|') && !line.starts_with("|---") && !line.contains("File") {
            found_table = true;
            let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            match cells.get(4).map(|s| s.to_lowercase()).as_deref() {
                Some("done") => done += 1,
                Some("pending") => pending += 1,
                _ => {}
            }
        }
    }
    if found_table {
        Some((done, pending))
    } else {
        None
    }
}
