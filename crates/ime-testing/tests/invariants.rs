//! Property tests for the engine invariants (spec §7.7, §22.3).

use std::sync::Arc;

use ime_core::{
    ContextInfo, Engine, EngineAction, EngineConfig, KeyInput, LogicalKey, Notify, Output, Scheme,
    MAX_COMPOSITION,
};
use ime_testing::FakeHost;
use proptest::prelude::*;

#[derive(Clone, Debug)]
enum Ev {
    Char(char),
    Key(LogicalKey),
    Notify(Notify),
}

fn arb_ev() -> impl Strategy<Value = Ev> {
    prop_oneof![
        8 => proptest::sample::select(
            "abcdefghijklmnopqrstuvwxyzABDEUOW0123456789[].,;'`~?^+(\\- "
                .chars()
                .collect::<Vec<_>>()
        )
        .prop_map(Ev::Char),
        1 => proptest::sample::select(vec![
            LogicalKey::Backspace,
            LogicalKey::Delete,
            LogicalKey::Enter,
            LogicalKey::Tab,
            LogicalKey::Escape,
            LogicalKey::Left,
            LogicalKey::Right,
            LogicalKey::Home,
            LogicalKey::Other,
        ])
        .prop_map(Ev::Key),
        1 => proptest::sample::select(vec![
            Notify::FocusOut,
            Notify::CaretMoved,
            Notify::SelectionChanged,
            Notify::CompositionTerminatedByHost,
            Notify::HostCommitFailed,
            Notify::Reset,
        ])
        .prop_map(Ev::Notify),
    ]
}

fn arb_scheme() -> impl Strategy<Value = Scheme> {
    prop_oneof![Just(Scheme::Telex), Just(Scheme::Vni), Just(Scheme::Viqr)]
}

fn input(id: u64, e: &Ev) -> Option<KeyInput> {
    match e {
        Ev::Char(c) => Some(KeyInput::character(id, *c)),
        Ev::Key(k) => Some(KeyInput::new(id, *k)),
        Ev::Notify(_) => None,
    }
}

fn apply(e: &mut Engine, id: u64, ev: &Ev) -> Output {
    match input(id, ev) {
        Some(k) => e.process(k),
        None => {
            let Ev::Notify(n) = ev else { unreachable!() };
            e.notify(id, *n)
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]

    /// I4 + I1 + host contract: no panic, `would_consume` agrees with `process`, nothing outside
    /// the composition is touched, and after focus loss nothing stays pending.
    #[test]
    fn engine_never_breaks_the_host_contract(
        scheme in arb_scheme(),
        evs in proptest::collection::vec(arb_ev(), 0..60),
    ) {
        let cfg = EngineConfig { scheme, ..EngineConfig::default() };
        let mut host = FakeHost::new(cfg);
        for ev in &evs {
            match ev {
                Ev::Char(c) => {
                    let mut s = String::new();
                    // `{` starts a token in key scripts; none of the generated chars is `{`.
                    s.push(*c);
                    host.feed(&s).unwrap();
                }
                Ev::Key(k) => host.step(ime_testing::Step::Key { logical: *k, text: None }),
                Ev::Notify(n) => host.step(ime_testing::Step::Notify(*n)),
            }
        }
        host.feed("{FOCUSOUT}").unwrap();
        prop_assert!(host.violations.is_empty(), "{:?}", host.violations);
        prop_assert!(host.preedit.is_none());
        prop_assert!(ime_unicode::is_nfc(&host.doc));
    }

    /// I3: the composition never exceeds `MAX_COMPOSITION` graphemes of preedit.
    #[test]
    fn preedit_is_bounded(scheme in arb_scheme(), word in "[a-zA-Z0-9]{0,200}") {
        let cfg = EngineConfig { scheme, ..EngineConfig::default() };
        let mut e = Engine::new(Arc::new(cfg));
        let id = e.create_context(ContextInfo::default());
        for c in word.chars() {
            let out = e.process(KeyInput::character(id, c));
            for a in &out.actions {
                if let EngineAction::UpdatePreedit { caret, .. } = a {
                    prop_assert!(caret.graphemes as usize <= MAX_COMPOSITION, "{}", caret.graphemes);
                }
            }
        }
    }

    /// Letters that no scheme turns into a mark come out unchanged, whatever the case.
    #[test]
    fn unmarked_text_is_identity(
        scheme in arb_scheme(),
        text in "[bBcCgGhHkKlLmMnNpPqQtTvV ]{0,60}",
    ) {
        let cfg = EngineConfig { scheme, ..EngineConfig::default() };
        let mut host = FakeHost::new(cfg);
        host.feed(&text).unwrap();
        host.feed("{FOCUSOUT}").unwrap();
        prop_assert_eq!(host.visible(), text);
    }

    /// Contexts are independent: what happens in B never changes what A reports.
    #[test]
    fn contexts_do_not_influence_each_other(
        scheme in arb_scheme(),
        script in proptest::collection::vec((any::<bool>(), arb_ev()), 0..80),
    ) {
        let cfg = Arc::new(EngineConfig { scheme, ..EngineConfig::default() });
        let mut both = Engine::new(Arc::clone(&cfg));
        let a = both.create_context(ContextInfo::default());
        let b = both.create_context(ContextInfo::default());
        let mut alone = Engine::new(cfg);
        let a_alone = alone.create_context(ContextInfo::default());

        for (to_a, ev) in &script {
            if *to_a {
                let with_b = apply(&mut both, a, ev);
                let without = apply(&mut alone, a_alone, ev);
                prop_assert_eq!(with_b, without);
            } else {
                apply(&mut both, b, ev);
            }
        }
    }

    /// I1 on its own, checked directly on the engine (the host check above also covers it).
    #[test]
    fn would_consume_matches_process(
        scheme in arb_scheme(),
        evs in proptest::collection::vec(arb_ev(), 0..60),
    ) {
        let cfg = EngineConfig { scheme, ..EngineConfig::default() };
        let mut e = Engine::new(Arc::new(cfg));
        let id = e.create_context(ContextInfo::default());
        for ev in &evs {
            if let Some(k) = input(id, ev) {
                let predicted = e.would_consume(&k);
                let out = e.process(k);
                prop_assert_eq!(predicted, out.consumed);
            } else {
                apply(&mut e, id, ev);
            }
        }
    }
}
