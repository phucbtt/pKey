//! Vowel-cluster (nucleus) + final-consonant table. See docs/IME_RULES.md.
//!
//! Clusters are given as display characters (`ê`, `ơ`, ...) without tone marks.
//! Entries without `""` in their final list need a final consonant to be complete.

use crate::Validity;

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

fn strip_char(c: char) -> char {
    match c {
        'ă' | 'â' => 'a',
        'ê' => 'e',
        'ô' | 'ơ' => 'o',
        'ư' => 'u',
        c => c,
    }
}

/// A typed vowel fits a table vowel when they are equal, or when the typed one is still unmarked
/// and the table vowel is its marked form (a mark may be added later).
fn vowel_fits(typed: char, table: char) -> bool {
    typed == table || typed == strip_char(table)
}

/// Classify `nucleus` + `coda` (both display text, non-empty nucleus).
pub(crate) fn classify(nucleus: &str, coda: &str) -> Validity {
    if RHYMES
        .iter()
        .any(|(c, codas)| *c == nucleus && codas.contains(&coda))
    {
        return Validity::Valid;
    }
    // Could become valid by typing more letters or by adding a shape mark to unmarked vowels.
    let typed: Vec<char> = nucleus.chars().collect();
    let can_extend = RHYMES.iter().any(|(c, codas)| {
        let table: Vec<char> = c.chars().collect();
        if typed.len() > table.len() || !typed.iter().zip(&table).all(|(t, e)| vowel_fits(*t, *e)) {
            return false;
        }
        if typed.len() == table.len() {
            codas.iter().any(|f| f.starts_with(coda))
        } else {
            // more vowels may follow, but no final consonant yet
            coda.is_empty()
        }
    });
    if can_extend {
        Validity::Prefix
    } else {
        Validity::Invalid
    }
}
