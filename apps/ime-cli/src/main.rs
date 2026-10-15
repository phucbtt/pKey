//! `pkey-cli` developer CLI: run the engine against a fake host and check golden files.
#![forbid(unsafe_code)]

use std::process::ExitCode;

use ime_core::{BackspaceMode, EngineConfig, OutputEncoding, Scheme, ToneStyle};
use ime_testing::{golden, FakeHost};

const USAGE: &str = "\
pkey-cli - pKey developer tool

USAGE:
  pkey-cli simulate [OPTIONS] <KEYS>     type KEYS into a fake text field and print the result
  pkey-cli golden <FILE>...              run golden files (see tests/golden), exit 1 on failures
  pkey-cli config default                print the default config.toml
  pkey-cli config validate <FILE>        check a config file, exit 1 on errors
  pkey-cli config migrate <FILE> [--write]   upgrade to the current schema (stdout, or in place)

SIMULATE OPTIONS:
  --scheme telex|vni|viqr   typing scheme (default telex)
  --tone-style modern|traditional
  --nfd                     output decomposed Unicode
  --no-spell-check          do not validate syllables
  --raw-backspace           Backspace undoes the last typed key
  --trace                   also print the engine actions for every step

KEYS may contain {BS} {ESC} {ENTER} {TAB} {LEFT} {RIGHT} {FOCUSOUT} ... (see ime-testing docs).
Exit codes: 0 ok, 1 golden failures, 2 usage error.
";

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("error: {msg}\n\n{USAGE}");
    ExitCode::from(2)
}

fn simulate(args: &[String]) -> ExitCode {
    let mut cfg = EngineConfig::default();
    let mut trace = false;
    let mut keys: Option<&str> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--scheme" => match it.next().map(String::as_str) {
                Some("telex") => cfg.scheme = Scheme::Telex,
                Some("vni") => cfg.scheme = Scheme::Vni,
                Some("viqr") => cfg.scheme = Scheme::Viqr,
                _ => return usage_error("--scheme needs telex|vni|viqr"),
            },
            "--tone-style" => match it.next().map(String::as_str) {
                Some("modern") => cfg.tone_style = ToneStyle::Modern,
                Some("traditional") => cfg.tone_style = ToneStyle::Traditional,
                _ => return usage_error("--tone-style needs modern|traditional"),
            },
            "--nfd" => cfg.encoding = OutputEncoding::UnicodeNfd,
            "--no-spell-check" => cfg.spell_check = false,
            "--raw-backspace" => cfg.backspace_mode = BackspaceMode::RawKey,
            "--trace" => trace = true,
            s if s.starts_with("--") => return usage_error(&format!("unknown option {s}")),
            s => {
                if keys.replace(s).is_some() {
                    return usage_error("only one KEYS argument is allowed (quote it)");
                }
            }
        }
    }
    let Some(keys) = keys else {
        return usage_error("missing KEYS");
    };
    let steps = match ime_testing::parse_keys(keys) {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    let mut host = FakeHost::new(cfg);
    for step in steps {
        host.step(step);
        if trace {
            println!("{step:?} -> {:?}", host.visible());
        }
    }
    println!("{}", host.visible());
    if !host.violations.is_empty() {
        eprintln!("host contract violations: {:?}", host.violations);
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run_golden(files: &[String]) -> ExitCode {
    if files.is_empty() {
        return usage_error("golden needs at least one file");
    }
    let (mut total, mut failed) = (0, 0);
    for path in files {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => return usage_error(&format!("{path}: {e}")),
        };
        let report = match golden::run_str(&src) {
            Ok(r) => r,
            Err(e) => return usage_error(&format!("{path}: {e}")),
        };
        total += report.total;
        failed += report.failures.len();
        for f in &report.failures {
            println!(
                "FAIL {path}:{} {}: keys={:?} expected={:?} actual={:?} {:?}",
                f.line, f.id, f.keys, f.expected, f.actual, f.violations
            );
        }
    }
    println!("{} passed, {failed} failed, {total} total", total - failed);
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_config(args: &[String]) -> ExitCode {
    use ime_config::{Config, ConfigError};

    let read = |path: &str| {
        std::fs::read_to_string(path).map_err(|e| usage_error(&format!("{path}: {e}")))
    };
    match args.first().map(String::as_str) {
        Some("default") => {
            print!("{}", Config::default().render(None));
            ExitCode::SUCCESS
        }
        Some("validate") => {
            let Some(path) = args.get(1) else {
                return usage_error("config validate needs a file");
            };
            let text = match read(path) {
                Ok(t) => t,
                Err(code) => return code,
            };
            match Config::parse(&text) {
                Ok(l) => {
                    for w in &l.warnings {
                        println!("warning: {w}");
                    }
                    if let Some(v) = l.migrated_from {
                        println!("note: file uses schema version {v} and would be migrated");
                    }
                    println!("ok");
                    ExitCode::SUCCESS
                }
                Err(ConfigError::Invalid(issues)) => {
                    for i in issues {
                        println!("error: {i}");
                    }
                    ExitCode::from(1)
                }
                Err(e) => {
                    println!("error: {e}");
                    ExitCode::from(1)
                }
            }
        }
        // Rewrites the file in the current schema (comments and unknown keys are kept).
        Some("migrate") => {
            let Some(path) = args.get(1) else {
                return usage_error("config migrate needs a file");
            };
            let write = args.get(2).map(String::as_str) == Some("--write");
            let text = match read(path) {
                Ok(t) => t,
                Err(code) => return code,
            };
            let loaded = match Config::parse(&text) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(1);
                }
            };
            let out = loaded.config.render(Some(&text));
            if write {
                if let Err(e) = ime_config::save_atomic(std::path::Path::new(path), &loaded.config)
                {
                    eprintln!("error: {path}: {e}");
                    return ExitCode::from(1);
                }
                println!("written {path}");
            } else {
                print!("{out}");
            }
            ExitCode::SUCCESS
        }
        _ => usage_error("expected: config default | validate <file> | migrate <file> [--write]"),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("simulate") => simulate(&args[1..]),
        Some("golden") => run_golden(&args[1..]),
        Some("config") => run_config(&args[1..]),
        Some("--help" | "-h" | "help") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => usage_error("expected a command: simulate | golden"),
    }
}
