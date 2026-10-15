//! Vowel-cluster (nucleus) + final-consonant table. See docs/IME_RULES.md.
//!
//! Clusters are given as display characters (`ê`, `ơ`, ...) without tone marks.
//! Entries without `""` in their final list need a final consonant to be complete.

use crate::{strip_shape, Validity};

const STOP_AND_NASAL: &[&str] = &["c", "m", "n", "ng", "p", "t"];

pub(crate) const RHYMES: &[(&str, &[&str])] = &[
    ("a", &["", "c", "ch", "m", "n", "ng", "nh", "p", "t"]),
    ("ă", &["c", "m", "n", "ng", "p", "t"]),
    ("â", &["c", "m", "n", "ng", "p", "t"]),
    ("e", &["", "c", "ch", "m", "n", "ng", "p", "t"]),
    ("ê", &["", "ch", "m", "n", "nh", "p", "t"]),
    ("i", &["", "ch", "m", "n", "nh", "p", "t"]),
    ("y", &[""]),
    ("o", &["", "c", "m", "n", "ng", "p", "t"]),
    ("ô", &["", "c", "m", "n", "ng", "p", "t"]),
    ("ơ", &["", "m", "n", "p", "t"]),
    ("u", &["", "c", "m", "n", "ng", "p", "t"]),
    ("ư", &["", "c", "ng", "t"]),
    ("ia", &[""]),
    ("iê", STOP_AND_NASAL),
    ("yê", &["m", "n", "ng", "t"]),
    ("ua", &[""]),
    ("uô", &["c", "m", "n", "ng", "t"]),
    ("ưa", &[""]),
    ("ươ", STOP_AND_NASAL),
    ("oa", &["", "c", "ch", "m", "n", "ng", "nh", "p", "t"]),
    ("oă", &["c", "m", "n", "ng", "t"]),
    ("oe", &["", "n", "t"]),
    ("uâ", &["n", "ng", "t"]),
    ("uê", &["", "ch", "nh"]),
    ("uy", &["", "ch", "nh", "n", "p", "t"]),
    ("uơ", &[""]),
    ("uya", &[""]),
    ("uyê", &["n", "t"]),
    // clusters that end in a semivowel (i, y, o, u)
    ("ai", &[""]),
    ("ao", &[""]),
    ("au", &[""]),
    ("ay", &[""]),
    ("âu", &[""]),
    ("ây", &[""]),
    ("eo", &[""]),
    ("êu", &[""]),
    ("iu", &[""]),
    ("oi", &[""]),
    ("ôi", &[""]),
    ("ơi", &[""]),
    ("ui", &[""]),
    ("ưi", &[""]),
    ("ưu", &[""]),
    ("iêu", &[""]),
    ("yêu", &[""]),
    ("oai", &[""]),
    ("oay", &[""]),
    ("uôi", &[""]),
    ("ươi", &[""]),
    ("ươu", &[""]),
    ("uây", &[""]),
    ("oeo", &[""]),
    ("uyu", &[""]),
    ("uêu", &[""]),
];

/// Classify `nucleus` + `coda` (both display text, non-empty nucleus).
pub(crate) fn classify(nucleus: &str, coda: &str) -> Validity {
    if RHYMES
        .iter()
        .any(|(c, codas)| *c == nucleus && codas.contains(&coda))
    {
        return Validity::Valid;
    }
    // Could become valid by typing more letters or by adding a shape mark.
    let sn = strip_shape(nucleus);
    let can_extend = RHYMES.iter().any(|(c, codas)| {
        let se = strip_shape(c);
        if se == sn {
            codas.iter().any(|f| f.starts_with(coda))
        } else {
            se.starts_with(&sn) && coda.is_empty()
        }
    });
    if can_extend {
        Validity::Prefix
    } else {
        Validity::Invalid
    }
}
