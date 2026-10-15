//! Golden test files: one case per line.
//!
//! ```text
//! # comment
//! TX001 telex "tieengs " => "tiếng "
//! TX010 telex tone_style=traditional "hoaf " => "hòa "
//! ```
//!
//! `ID SCHEME [option=value ...] "keys" => "expected"`. Strings accept `\"`, `\\`, `\n`, `\t`
//! and `\u{XXXX}`. The result is the committed text plus any composition still shown.
//! Options: `tone_style=modern|traditional`, `spell_check`, `free_marking`, `auto_restore`,
//! `w_standalone`, `brackets`, `foreign_onsets`, `focus_commit` (all `true|false`),
//! `restore_key=esc|none`, `backspace=grapheme|raw`, `output=nfc|nfd`.

use ime_core::{BackspaceMode, EngineConfig, RestoreKey, Scheme, ToneStyle};

use crate::host::FakeHost;

#[derive(Debug, Clone)]
pub struct Case {
    pub id: String,
    pub config: EngineConfig,
    pub keys: String,
    pub expected: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Failure {
    pub id: String,
    pub line: usize,
    pub keys: String,
    pub expected: String,
    pub actual: String,
    pub violations: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Report {
    pub total: usize,
    pub failures: Vec<Failure>,
}

fn parse_bool(v: &str) -> Result<bool, String> {
    match v {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("expected true|false, got '{v}'")),
    }
}

fn apply_option(cfg: &mut EngineConfig, key: &str, value: &str) -> Result<(), String> {
    match key {
        "tone_style" => {
            cfg.tone_style = match value {
                "modern" => ToneStyle::Modern,
                "traditional" => ToneStyle::Traditional,
                _ => return Err(format!("bad tone_style '{value}'")),
            }
        }
        "spell_check" => cfg.spell_check = parse_bool(value)?,
        "free_marking" => cfg.free_marking = parse_bool(value)?,
        "auto_restore" => cfg.auto_restore_invalid = parse_bool(value)?,
        "w_standalone" => cfg.telex_w_standalone = parse_bool(value)?,
        "brackets" => cfg.telex_brackets = parse_bool(value)?,
        "foreign_onsets" => cfg.allow_foreign_onsets = parse_bool(value)?,
        "focus_commit" => cfg.commit_on_focus_loss = parse_bool(value)?,
        "restore_key" => {
            cfg.restore_key = match value {
                "esc" => RestoreKey::Esc,
                "none" => RestoreKey::None,
                _ => return Err(format!("bad restore_key '{value}'")),
            }
        }
        "backspace" => {
            cfg.backspace_mode = match value {
                "grapheme" => BackspaceMode::Grapheme,
                "raw" => BackspaceMode::RawKey,
                _ => return Err(format!("bad backspace '{value}'")),
            }
        }
        "output" => {
            cfg.encoding = match value {
                "nfc" => ime_core::OutputEncoding::UnicodeNfc,
                "nfd" => ime_core::OutputEncoding::UnicodeNfd,
                _ => return Err(format!("bad output '{value}'")),
            }
        }
        _ => return Err(format!("unknown option '{key}'")),
    }
    Ok(())
}

/// Parse a quoted string starting at `s[0] == '"'`; returns the value and the rest after it.
fn parse_quoted(s: &str) -> Result<(String, &str), String> {
    let mut chars = s.char_indices();
    match chars.next() {
        Some((_, '"')) => {}
        _ => return Err("expected opening quote".into()),
    }
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &s[i + 1..])),
            '\\' => match chars.next() {
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, 'u')) => {
                    if chars.next().map(|(_, c)| c) != Some('{') {
                        return Err("expected '{' after \\u".into());
                    }
                    let mut hex = String::new();
                    loop {
                        match chars.next() {
                            Some((_, '}')) => break,
                            Some((_, h)) => hex.push(h),
                            None => return Err("unterminated \\u{...}".into()),
                        }
                    }
                    let cp = u32::from_str_radix(&hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or_else(|| format!("bad code point '{hex}'"))?;
                    out.push(cp);
                }
                other => return Err(format!("bad escape {other:?}")),
            },
            c => out.push(c),
        }
    }
    Err("unterminated string".into())
}

fn parse_line(line: &str, line_no: usize) -> Result<Case, String> {
    let mut rest = line.trim_start();
    let take_word = |rest: &mut &str| -> Option<String> {
        let end = rest.find(char::is_whitespace)?;
        let word = rest[..end].to_string();
        *rest = rest[end..].trim_start();
        Some(word)
    };
    let id = take_word(&mut rest).ok_or("missing id")?;
    let scheme = take_word(&mut rest).ok_or("missing scheme")?;
    let mut config = EngineConfig {
        scheme: match scheme.as_str() {
            "telex" => Scheme::Telex,
            "vni" => Scheme::Vni,
            "viqr" => Scheme::Viqr,
            other => return Err(format!("unknown scheme '{other}'")),
        },
        ..EngineConfig::default()
    };
    while !rest.starts_with('"') {
        let word = take_word(&mut rest).ok_or("missing keys string")?;
        let (k, v) = word
            .split_once('=')
            .ok_or_else(|| format!("expected option key=value, got '{word}'"))?;
        apply_option(&mut config, k, v)?;
    }
    let (keys, after) = parse_quoted(rest)?;
    let after = after.trim_start();
    let after = after.strip_prefix("=>").ok_or("missing '=>'")?.trim_start();
    let (expected, tail) = parse_quoted(after)?;
    if !tail.trim().is_empty() {
        return Err(format!("trailing text '{}'", tail.trim()));
    }
    Ok(Case {
        id,
        config,
        keys,
        expected,
        line: line_no,
    })
}

pub fn parse(src: &str) -> Result<Vec<Case>, String> {
    let mut cases = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        cases.push(parse_line(line, i + 1).map_err(|e| format!("line {}: {e}", i + 1))?);
    }
    Ok(cases)
}

pub fn run_case(case: &Case) -> Result<(), Box<Failure>> {
    let mut host = FakeHost::new(case.config.clone());
    let fail = |actual: String, violations: Vec<String>| {
        Box::new(Failure {
            id: case.id.clone(),
            line: case.line,
            keys: case.keys.clone(),
            expected: case.expected.clone(),
            actual,
            violations,
        })
    };
    if let Err(e) = host.feed(&case.keys) {
        return Err(fail(format!("<bad key script: {e}>"), Vec::new()));
    }
    let actual = host.visible();
    if actual != case.expected || !host.violations.is_empty() {
        return Err(fail(actual, host.violations.clone()));
    }
    Ok(())
}

pub fn run_str(src: &str) -> Result<Report, String> {
    let cases = parse(src)?;
    let mut report = Report {
        total: cases.len(),
        failures: Vec::new(),
    };
    for case in &cases {
        if let Err(f) = run_case(case) {
            report.failures.push(*f);
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options_and_escapes() {
        let cases = parse(
            "# c\n\nA1 telex tone_style=traditional spell_check=false \"a\\\"b\\u{1EA5}\" => \"x\\n\"\n",
        )
        .unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].keys, "a\"bấ");
        assert_eq!(cases[0].expected, "x\n");
        assert_eq!(cases[0].config.tone_style, ToneStyle::Traditional);
        assert!(!cases[0].config.spell_check);
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse("A1 telex \"a\" \"b\"").is_err());
        assert!(parse("A1 nope \"a\" => \"b\"").is_err());
        assert!(parse("A1 telex bogus=1 \"a\" => \"b\"").is_err());
        assert!(parse("A1 telex \"a\" => \"b\" extra").is_err());
    }

    #[test]
    fn reports_failures() {
        let r = run_str("OK telex \"xin \" => \"xin \"\nBAD telex \"xin \" => \"nope\"\n").unwrap();
        assert_eq!(r.total, 2);
        assert_eq!(r.failures.len(), 1);
        assert_eq!(r.failures[0].id, "BAD");
    }
}
