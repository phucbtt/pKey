//! `macros.json` and CSV import/export (spec §11.1, §11.2).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{validate, Issue, Macro, Scope, MAX_EXPANSION_CHARS};

pub const SCHEMA_VERSION: u32 = 1;
/// Rotating backups kept before an import or a save replaces `macros.json`.
pub const BACKUP_COUNT: usize = 5;

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ScopeDto {
    Global,
    Profile { profile_id: String },
}

#[derive(Serialize, Deserialize)]
struct MacroDto {
    id: String,
    trigger: String,
    expansion: String,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default = "yes")]
    whole_word: bool,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default = "default_scope")]
    scope: ScopeDto,
    #[serde(default)]
    priority: i32,
    #[serde(default = "yes")]
    case_follow: bool,
}

fn default_scope() -> ScopeDto {
    ScopeDto::Global
}

#[derive(Serialize, Deserialize)]
struct FileDto {
    schema_version: u32,
    macros: Vec<MacroDto>,
}

impl From<MacroDto> for Macro {
    fn from(d: MacroDto) -> Macro {
        Macro {
            id: d.id,
            trigger: d.trigger,
            expansion: d.expansion,
            case_sensitive: d.case_sensitive,
            whole_word: d.whole_word,
            enabled: d.enabled,
            scope: match d.scope {
                ScopeDto::Global => Scope::Global,
                ScopeDto::Profile { profile_id } => Scope::Profile(profile_id),
            },
            priority: d.priority,
            case_follow: d.case_follow,
        }
    }
}

impl From<&Macro> for MacroDto {
    fn from(m: &Macro) -> MacroDto {
        MacroDto {
            id: m.id.clone(),
            trigger: m.trigger.clone(),
            expansion: m.expansion.clone(),
            case_sensitive: m.case_sensitive,
            whole_word: m.whole_word,
            enabled: m.enabled,
            scope: match &m.scope {
                Scope::Global => ScopeDto::Global,
                Scope::Profile(id) => ScopeDto::Profile {
                    profile_id: id.clone(),
                },
            },
            priority: m.priority,
            case_follow: m.case_follow,
        }
    }
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    /// Not valid JSON, or not the expected shape. Carries the parser message (no macro text).
    Format(String),
    /// A newer file than this build understands.
    NewerSchema(u32),
    Invalid(Vec<Issue>),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "macros I/O error: {e}"),
            StoreError::Format(e) => write!(f, "macros file format error: {e}"),
            StoreError::NewerSchema(v) => write!(f, "macros file has newer schema version {v}"),
            StoreError::Invalid(i) => write!(f, "{} invalid macro entries", i.len()),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

/// Parse `macros.json` text. The result is validated.
pub fn from_json(text: &str, max_expansion: usize) -> Result<Vec<Macro>, StoreError> {
    let file: FileDto = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| StoreError::Format(e.to_string()))?;
    if file.schema_version > SCHEMA_VERSION {
        return Err(StoreError::NewerSchema(file.schema_version));
    }
    let macros: Vec<Macro> = file.macros.into_iter().map(Macro::from).collect();
    let issues = validate(&macros, max_expansion);
    if issues.is_empty() {
        Ok(macros)
    } else {
        Err(StoreError::Invalid(issues))
    }
}

pub fn to_json(macros: &[Macro]) -> String {
    let file = FileDto {
        schema_version: SCHEMA_VERSION,
        macros: macros.iter().map(MacroDto::from).collect(),
    };
    let mut s = serde_json::to_string_pretty(&file).expect("plain data always serializes");
    s.push('\n');
    s
}

/// A fresh random-looking id (UUID layout). Only uniqueness matters, not unpredictability.
pub fn new_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    // `RandomState` is seeded randomly per process, which keeps ids from two processes apart.
    let mut h = RandomState::new().build_hasher();
    h.write_u128(nanos);
    h.write_u64(n);
    let a = h.finish();
    h.write_u64(a);
    let b = h.finish();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        (a & 0xfff) as u16,
        ((b >> 48) as u16 & 0x3fff) | 0x8000,
        b & 0xffff_ffff_ffff
    )
}

// ---------------------------------------------------------------- CSV

type CsvRows = Vec<(usize, Vec<String>)>;

/// Split CSV text into rows of fields (RFC 4180: quotes, doubled quotes, newlines in quotes).
fn csv_rows(text: &str) -> Result<CsvRows, (usize, &'static str)> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut line = 1;
    let mut row_line = 1;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if in_quotes {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                '\r' => {}
                c => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_quotes = true,
            '"' => return Err((line, "quote in the middle of a field")),
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push((row_line, std::mem::take(&mut row)));
                line += 1;
                row_line = line;
                any = false;
            }
            c => field.push(c),
        }
    }
    if in_quotes {
        return Err((row_line, "unterminated quote"));
    }
    if any || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push((row_line, row));
    }
    Ok(rows)
}

fn csv_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// Result of reading a CSV: nothing is applied until the caller confirms the preview.
#[derive(Debug, Default)]
pub struct ImportPreview {
    pub macros: Vec<Macro>,
    /// 1-based line number and the problem (never the macro text).
    pub errors: Vec<(usize, String)>,
}

/// Columns: `trigger,expansion[,case_sensitive,whole_word,enabled]`, UTF-8 with or without BOM,
/// optional header row. Rows with problems are reported and skipped.
pub fn import_csv(text: &str, max_expansion: usize) -> ImportPreview {
    let mut preview = ImportPreview::default();
    let text = text.trim_start_matches('\u{feff}');
    let rows = match csv_rows(text) {
        Ok(r) => r,
        Err((line, msg)) => {
            preview.errors.push((line, msg.to_string()));
            return preview;
        }
    };
    let max_expansion = max_expansion.min(MAX_EXPANSION_CHARS);
    for (n, (line, fields)) in rows.into_iter().enumerate() {
        if fields.len() == 1 && fields[0].trim().is_empty() {
            continue;
        }
        if n == 0 && fields[0].trim().eq_ignore_ascii_case("trigger") {
            continue;
        }
        if fields.len() < 2 || fields.len() > 5 {
            preview
                .errors
                .push((line, "expected 2 to 5 columns".to_string()));
            continue;
        }
        let mut m = Macro::new(fields[0].trim(), &fields[1]);
        m.id = new_id();
        let mut bad_flag = false;
        for (i, slot) in [&mut m.case_sensitive, &mut m.whole_word, &mut m.enabled]
            .into_iter()
            .enumerate()
        {
            if let Some(f) = fields.get(2 + i).filter(|f| !f.trim().is_empty()) {
                match csv_bool(f) {
                    Some(b) => *slot = b,
                    None => bad_flag = true,
                }
            }
        }
        if bad_flag {
            preview
                .errors
                .push((line, "flag columns must be true or false".to_string()));
            continue;
        }
        let issues = validate(std::slice::from_ref(&m), max_expansion);
        if let Some(i) = issues.iter().find(|i| i.kind != crate::IssueKind::EmptyId) {
            preview.errors.push((line, format!("{:?}", i.kind)));
            continue;
        }
        preview.macros.push(m);
    }
    preview
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Export as CSV (global and profile macros alike; the scope is not part of the CSV format).
pub fn export_csv(macros: &[Macro]) -> String {
    let mut out = String::from("trigger,expansion,case_sensitive,whole_word,enabled\n");
    for m in macros {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            csv_field(&m.trigger),
            csv_field(&m.expansion),
            m.case_sensitive,
            m.whole_word,
            m.enabled
        ));
    }
    out
}

/// What importing `incoming` into `existing` would do.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MergePlan {
    pub added: usize,
    /// Same trigger already present in the global scope: skipped, the existing one is kept.
    pub skipped_duplicates: usize,
}

/// Merge global macros: new triggers are appended, triggers that already exist are skipped.
pub fn merge(existing: &mut Vec<Macro>, incoming: Vec<Macro>) -> MergePlan {
    let mut plan = MergePlan::default();
    for m in incoming {
        let probe = {
            let mut all = existing.clone();
            all.push(m.clone());
            validate(&all, MAX_EXPANSION_CHARS)
                .iter()
                .any(|i| i.index == all.len() - 1 && i.kind == crate::IssueKind::DuplicateTrigger)
        };
        if probe {
            plan.skipped_duplicates += 1;
        } else {
            existing.push(m);
            plan.added += 1;
        }
    }
    plan
}

// ---------------------------------------------------------------- files

pub fn load(path: &Path, max_expansion: usize) -> Result<Vec<Macro>, StoreError> {
    match fs::read_to_string(path) {
        Ok(t) => from_json(&t, max_expansion),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

fn backup(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let dir = path.parent().unwrap_or(Path::new(".")).join("backups");
    fs::create_dir_all(&dir)?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("macros");
    let mut n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let target = loop {
        let t = dir.join(format!("{stem}-{n:016}.json"));
        if !t.exists() {
            break t;
        }
        n += 1;
    };
    fs::copy(path, target)?;
    let prefix = format!("{stem}-");
    let mut all: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".json"))
        })
        .collect();
    all.sort();
    let excess = all.len().saturating_sub(BACKUP_COUNT);
    for p in all.into_iter().take(excess) {
        let _ = fs::remove_file(p);
    }
    Ok(())
}

/// Save with a backup of the previous file and an atomic replace (temp file + fsync + rename).
pub fn save_atomic(path: &Path, macros: &[Macro]) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let text = to_json(macros);
    if fs::read_to_string(path).ok().as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    backup(path)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut f = File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IssueKind;

    fn sample() -> Vec<Macro> {
        let mut a = Macro::new("vn", "Việt Nam");
        a.id = new_id();
        let mut b = Macro::new("sig", "Trân trọng,\n\"Tên\"");
        b.id = new_id();
        b.scope = Scope::Profile("p1".into());
        b.priority = 3;
        b.case_sensitive = true;
        b.whole_word = false;
        b.enabled = false;
        b.case_follow = false;
        vec![a, b]
    }

    #[test]
    fn json_round_trip() {
        let list = sample();
        let text = to_json(&list);
        assert!(text.contains("\"schema_version\": 1"));
        assert!(text.contains("\"type\": \"profile\""));
        assert_eq!(from_json(&text, 4096).unwrap(), list);
    }

    #[test]
    fn json_defaults_for_missing_fields() {
        let text = r#"{"schema_version":1,"macros":[{"id":"1","trigger":"vn","expansion":"x"}]}"#;
        let m = &from_json(text, 100).unwrap()[0];
        assert!(m.whole_word && m.enabled && m.case_follow && !m.case_sensitive);
        assert_eq!(m.scope, Scope::Global);
    }

    #[test]
    fn json_errors() {
        assert!(matches!(from_json("{", 10), Err(StoreError::Format(_))));
        assert!(matches!(
            from_json(r#"{"schema_version":2,"macros":[]}"#, 10),
            Err(StoreError::NewerSchema(2))
        ));
        let dup = r#"{"schema_version":1,"macros":[
            {"id":"1","trigger":"a","expansion":"x"},{"id":"2","trigger":"A","expansion":"y"}]}"#;
        match from_json(dup, 10) {
            Err(StoreError::Invalid(i)) => assert_eq!(i[0].kind, IssueKind::DuplicateTrigger),
            other => panic!("{other:?}"),
        }
        // The error text must not echo macro content.
        let msg = from_json(r#"{"schema_version":1,"macros":[{"id":"SECRETID"}]}"#, 10)
            .unwrap_err()
            .to_string();
        assert!(!msg.contains("SECRETID"), "{msg}");
    }

    #[test]
    fn bom_is_accepted() {
        let text = format!("\u{feff}{}", to_json(&sample()));
        assert!(from_json(&text, 4096).is_ok());
    }

    #[test]
    fn csv_round_trip_with_awkward_text() {
        let mut a = Macro::new("vn", "Việt, \"Nam\"\nline2");
        a.id = new_id();
        let csv = export_csv(&[a.clone()]);
        let p = import_csv(&csv, 4096);
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        assert_eq!(p.macros.len(), 1);
        assert_eq!(p.macros[0].trigger, a.trigger);
        assert_eq!(p.macros[0].expansion, a.expansion);
        assert!(!p.macros[0].id.is_empty());
    }

    #[test]
    fn csv_import_reports_bad_rows_and_keeps_good_ones() {
        let csv = "\u{feff}trigger,expansion,case_sensitive\r\n\
                   vn,Việt Nam\r\n\
                   ,empty trigger\r\n\
                   one-column\r\n\
                   ok,fine,maybe\r\n\
                   good,fine,true,false,false\r\n\
                   \r\n\
                   a b,spaces\r\n";
        let p = import_csv(csv, 4096);
        let triggers: Vec<_> = p.macros.iter().map(|m| m.trigger.as_str()).collect();
        assert_eq!(triggers, ["vn", "good"]);
        let lines: Vec<usize> = p.errors.iter().map(|e| e.0).collect();
        assert_eq!(lines, [3, 4, 5, 8]);
        let good = &p.macros[1];
        assert!(good.case_sensitive && !good.whole_word && !good.enabled);
        // Error text names the problem, not the macro.
        assert!(p.errors.iter().all(|e| !e.1.contains("spaces")));
    }

    #[test]
    fn csv_unterminated_quote_is_an_error() {
        let p = import_csv("a,\"oops\n", 100);
        assert!(p.macros.is_empty());
        assert_eq!(p.errors.len(), 1);
    }

    #[test]
    fn merge_skips_existing_triggers() {
        let mut existing = sample();
        let mut dup = Macro::new("VN", "other");
        dup.id = new_id();
        let mut fresh = Macro::new("hcm", "Hồ Chí Minh");
        fresh.id = new_id();
        let plan = merge(&mut existing, vec![dup, fresh]);
        assert_eq!(
            plan,
            MergePlan {
                added: 1,
                skipped_duplicates: 1
            }
        );
        assert_eq!(existing.len(), 3);
    }

    #[test]
    fn ids_are_unique() {
        let ids: std::collections::HashSet<_> = (0..1000).map(|_| new_id()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(ids
            .iter()
            .all(|i| i.len() == 36 && i.as_bytes()[14] == b'4'));
    }

    #[test]
    fn save_load_and_backup_rotation() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("macros.json");
        assert!(load(&p, 100).unwrap().is_empty());
        let mut list = vec![{
            let mut m = Macro::new("a", "1");
            m.id = new_id();
            m
        }];
        save_atomic(&p, &list).unwrap();
        assert_eq!(load(&p, 100).unwrap(), list);
        for i in 0..(BACKUP_COUNT + 3) {
            list[0].expansion = format!("v{i}");
            save_atomic(&p, &list).unwrap();
        }
        let n = fs::read_dir(d.path().join("backups")).unwrap().count();
        assert_eq!(n, BACKUP_COUNT);
        // Saving the same content again changes nothing.
        save_atomic(&p, &list).unwrap();
        assert_eq!(
            fs::read_dir(d.path().join("backups")).unwrap().count(),
            BACKUP_COUNT
        );
    }
}
