//! Schema migrations (spec §16.4): a chain of `vN -> vN+1` functions that edit the TOML document
//! in place, so unknown keys and comments survive.

use toml_edit::{value, DocumentMut};

pub type Migration = fn(&mut DocumentMut) -> Result<(), String>;

/// `MIGRATIONS[i]` upgrades schema version `i + 1` to `i + 2`. Empty while v1 is the only version;
/// when v2 is introduced, add `migrate_v1_to_v2` here together with a test.
pub const MIGRATIONS: &[Migration] = &[];

pub fn migrate_document(doc: &mut DocumentMut, from: i64, to: i64) -> Result<(), String> {
    run(doc, from, to, MIGRATIONS)
}

fn run(doc: &mut DocumentMut, from: i64, to: i64, steps: &[Migration]) -> Result<(), String> {
    let mut v = from;
    while v < to {
        let step = usize::try_from(v - 1)
            .ok()
            .and_then(|i| steps.get(i))
            .ok_or_else(|| format!("no migration from schema version {v}"))?;
        step(doc)?;
        v += 1;
        doc["schema_version"] = value(v);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v1_to_v2(doc: &mut DocumentMut) -> Result<(), String> {
        // Example: `general.mode` was renamed to `general.default_mode`.
        if let Some(old) = doc["general"]["mode"].as_str().map(str::to_owned) {
            doc["general"]["default_mode"] = value(old);
            doc["general"].as_table_mut().unwrap().remove("mode");
        }
        Ok(())
    }

    fn v2_to_v3(doc: &mut DocumentMut) -> Result<(), String> {
        doc["added_in_v3"] = value(true);
        Ok(())
    }

    #[test]
    fn steps_run_in_order_and_keep_unknown_keys() {
        let mut doc: DocumentMut = "schema_version = 1\nextra = 5\n[general]\nmode = \"en\"\n"
            .parse()
            .unwrap();
        run(&mut doc, 1, 3, &[v1_to_v2, v2_to_v3]).unwrap();
        assert_eq!(doc["schema_version"].as_integer(), Some(3));
        assert_eq!(doc["general"]["default_mode"].as_str(), Some("en"));
        assert!(doc["general"].as_table().unwrap().get("mode").is_none());
        assert_eq!(doc["extra"].as_integer(), Some(5));
        assert_eq!(doc["added_in_v3"].as_bool(), Some(true));
    }

    #[test]
    fn missing_step_is_an_error_and_current_version_is_a_noop() {
        let mut doc: DocumentMut = "schema_version = 1\n".parse().unwrap();
        assert!(run(&mut doc, 1, 2, &[]).is_err());
        run(&mut doc, 1, 1, &[]).unwrap();
        assert_eq!(doc["schema_version"].as_integer(), Some(1));
    }

    #[test]
    fn a_failing_step_stops_the_chain() {
        fn boom(_: &mut DocumentMut) -> Result<(), String> {
            Err("boom".into())
        }
        let mut doc: DocumentMut = "schema_version = 1\n".parse().unwrap();
        assert_eq!(run(&mut doc, 1, 3, &[boom, v2_to_v3]), Err("boom".into()));
        assert_eq!(doc["schema_version"].as_integer(), Some(1));
    }
}
