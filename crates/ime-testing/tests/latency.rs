//! Per-key latency budget (spec §21): `process_key` p95 must stay under 1 ms, also in a debug
//! build, so a regression shows up in every CI run. For release numbers run
//! `cargo test -p ime-testing --release --test latency -- --nocapture`.

use std::sync::Arc;
use std::time::Instant;

use ime_core::{ContextInfo, Engine, EngineConfig, KeyInput, Scheme};

fn percentile(sorted: &[u128], p: f64) -> u128 {
    sorted[((sorted.len() as f64 * p) as usize).min(sorted.len() - 1)]
}

fn measure(scheme: Scheme, text: &str) -> (u128, u128, u128) {
    let cfg = EngineConfig {
        scheme,
        ..EngineConfig::default()
    };
    let mut e = Engine::new(Arc::new(cfg));
    let id = e.create_context(ContextInfo::default());
    let mut times = Vec::new();
    for _ in 0..200 {
        for c in text.chars() {
            let ev = KeyInput::character(id, c);
            let t = Instant::now();
            let out = e.process(ev);
            times.push(t.elapsed().as_nanos());
            std::hint::black_box(out);
        }
    }
    times.sort_unstable();
    (
        percentile(&times, 0.50),
        percentile(&times, 0.95),
        percentile(&times, 0.99),
    )
}

#[test]
fn process_key_p95_is_under_one_millisecond() {
    for (scheme, text) in [
        (Scheme::Telex, "nguowif vieejt nam toois nay thuwowngf "),
        (Scheme::Vni, "ngu7o72i vie6.t nam to6i1 nay thu7o7ng2 "),
        (Scheme::Viqr, "ngu+o+`i vie^.t nam to^i' nay thu+o+ng` "),
    ] {
        let (p50, p95, p99) = measure(scheme, text);
        println!("{scheme:?}: p50 {p50} ns, p95 {p95} ns, p99 {p99} ns");
        assert!(p95 < 1_000_000, "{scheme:?} p95 = {p95} ns");
    }
}
