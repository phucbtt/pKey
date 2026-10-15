use std::fs;
use std::path::PathBuf;

use ime_core::EngineConfig;
use ime_testing::{golden, FakeHost};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden")
}

#[test]
fn golden_files_pass() {
    let mut total = 0;
    let mut failures = Vec::new();
    let mut files = 0;
    for entry in fs::read_dir(golden_dir()).expect("tests/golden exists") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("golden") {
            continue;
        }
        files += 1;
        let src = fs::read_to_string(&path).unwrap();
        let report = golden::run_str(&src)
            .unwrap_or_else(|e| panic!("{}: parse error: {e}", path.display()));
        total += report.total;
        for f in report.failures {
            failures.push(format!(
                "{}:{} {} keys={:?} expected={:?} actual={:?} violations={:?}",
                path.file_name().unwrap().to_string_lossy(),
                f.line,
                f.id,
                f.keys,
                f.expected,
                f.actual,
                f.violations
            ));
        }
    }
    assert!(files >= 2, "expected at least two golden files");
    assert!(
        failures.is_empty(),
        "{} of {total} golden cases failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Deterministic pseudo-random key scripts: the engine never panics and the host contract holds.
#[test]
fn random_scripts_keep_host_contract() {
    const TOKENS: &[&str] = &[
        "a",
        "e",
        "o",
        "u",
        "i",
        "y",
        "d",
        "w",
        "s",
        "f",
        "r",
        "x",
        "j",
        "z",
        "b",
        "c",
        "g",
        "h",
        "n",
        "t",
        "q",
        "A",
        "D",
        "S",
        "1",
        "2",
        "6",
        "7",
        "9",
        " ",
        ".",
        ",",
        "{BS}",
        "{ESC}",
        "{ENTER}",
        "{LEFT}",
        "{FOCUSOUT}",
        "{CARET}",
        "{HOSTCANCEL}",
    ];
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        // xorshift64*
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        seed.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };
    for scheme_vni in [false, true] {
        for _ in 0..3000 {
            let len = (next() % 40) as usize;
            let script: String = (0..len)
                .map(|_| TOKENS[(next() % TOKENS.len() as u64) as usize])
                .collect();
            let cfg = EngineConfig {
                scheme: if scheme_vni {
                    ime_core::Scheme::Vni
                } else {
                    ime_core::Scheme::Telex
                },
                ..EngineConfig::default()
            };
            let mut host = FakeHost::new(cfg);
            host.feed(&script).unwrap();
            host.feed("{FOCUSOUT}").unwrap();
            assert!(
                host.violations.is_empty(),
                "script {script:?}: {:?}",
                host.violations
            );
            assert!(host.preedit.is_none(), "script {script:?}");
            assert!(ime_unicode::is_nfc(&host.doc), "script {script:?}");
        }
    }
}

/// Keys that no scheme turns into a mark must come out unchanged.
#[test]
fn plain_consonants_are_identity() {
    let mut host = FakeHost::new(EngineConfig::default());
    host.feed("bcghklmnpqtv xin ").unwrap();
    assert_eq!(host.visible(), "bcghklmnpqtv xin ");
}
