//! IBus's serialised objects (`IBusText`, `IBusAttrList`, `IBusProperty`, ...). On the bus each
//! one is a variant that holds a structure `(s a{sv} ...)` whose first field is the type name.

use std::collections::HashMap;

use zbus::zvariant::{StructureBuilder, Value};

/// `IBusAttrType`.
const ATTR_TYPE_UNDERLINE: u32 = 1;
/// `IBusAttrUnderline`.
const UNDERLINE_SINGLE: u32 = 1;

/// `IBusPropType`.
const PROP_TYPE_NORMAL: u32 = 0;
/// `IBusPropState`.
const PROP_STATE_UNCHECKED: u32 = 0;

fn no_hints() -> Value<'static> {
    Value::new(HashMap::<String, Value<'static>>::new())
}

fn object(name: &str, fields: Vec<Value<'static>>) -> Value<'static> {
    let mut b = StructureBuilder::new()
        .add_field(name.to_string())
        .append_field(no_hints());
    for f in fields {
        b = b.append_field(f);
    }
    Value::Structure(b.build().expect("IBus object has fields"))
}

fn attribute(kind: u32, value: u32, start: u32, end: u32) -> Value<'static> {
    object(
        "IBusAttribute",
        vec![kind.into(), value.into(), start.into(), end.into()],
    )
}

fn attr_list(attrs: Vec<Value<'static>>) -> Value<'static> {
    // `av`: zvariant wraps each element in a variant itself.
    object("IBusAttrList", vec![Value::new(attrs)])
}

/// `IBusText`; `underline` marks the whole string as being composed. Offsets are in characters.
pub fn text(s: &str, underline: bool) -> Value<'static> {
    let attrs = if underline && !s.is_empty() {
        let n = s.chars().count() as u32;
        vec![attribute(ATTR_TYPE_UNDERLINE, UNDERLINE_SINGLE, 0, n)]
    } else {
        Vec::new()
    };
    object(
        "IBusText",
        vec![s.to_string().into(), Value::new(attr_list(attrs))],
    )
}

/// The "VI"/"EN" indicator, one normal (non-toggle) property that a click flips.
pub fn mode_property(label: &str, tooltip: &str) -> Value<'static> {
    object(
        "IBusProperty",
        vec![
            crate::MODE_PROPERTY.to_string().into(),
            PROP_TYPE_NORMAL.into(),
            Value::new(text(label, false)),
            String::new().into(),
            Value::new(text(tooltip, false)),
            true.into(),
            true.into(),
            PROP_STATE_UNCHECKED.into(),
            Value::new(prop_list(Vec::new())),
            // The symbol shown in the indicator (IBus 1.5.19 and later).
            Value::new(text(label, false)),
        ],
    )
}

fn prop_list(props: Vec<Value<'static>>) -> Value<'static> {
    object("IBusPropList", vec![Value::new(props)])
}

/// `RegisterProperties` payload.
pub fn registered_properties(props: Vec<Value<'static>>) -> Value<'static> {
    prop_list(props)
}

/// The string inside an `IBusText` variant (for `SetSurroundingText`).
pub fn text_of(v: &Value<'_>) -> Option<String> {
    let mut v = v;
    while let Value::Value(inner) = v {
        v = inner;
    }
    let Value::Structure(s) = v else { return None };
    match s.fields().get(2)? {
        Value::Str(t) => Some(t.to_string()),
        _ => None,
    }
}

fn strings(items: &[&str]) -> Vec<Value<'static>> {
    items.iter().map(|s| Value::from(s.to_string())).collect()
}

/// `IBusComponent` with the one pKey engine, for `RegisterComponent`. This lets the engine
/// announce itself to a running daemon, which is how it works without writing into
/// `/usr/share/ibus/component` (that directory is the only one the daemon reads).
pub fn component(command_line: &str, version: &str, icon: &str) -> Value<'static> {
    let mut engine = strings(&[
        "pkey",
        "pKey",
        "Telex, VNI, VIQR",
        "vi",
        "MIT",
        "pKey",
        icon,
    ]);
    engine.push("default".to_string().into()); // layout
    engine.push(50u32.into()); // rank
    engine.extend(strings(&["", "", "", "", "", version, "", ""]));
    let mut fields = strings(&[
        crate::COMPONENT_NAME,
        "pKey Vietnamese Input Method",
        version,
        "MIT",
        "pKey",
        "https://github.com/phucbtt/pKey",
        command_line,
        "pkey",
    ]);
    fields.push(Value::new(Vec::<Value<'static>>::new()));
    fields.push(Value::new(vec![object("IBusEngineDesc", engine)]));
    object("IBusComponent", fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(v: &Value<'_>) -> String {
        v.value_signature().to_string()
    }

    #[test]
    fn text_has_the_ibus_layout() {
        let v = text("việt", true);
        assert_eq!(signature(&v), "(sa{sv}sv)");
        let Value::Structure(s) = &v else { panic!() };
        assert_eq!(s.fields()[0], Value::new("IBusText"));
        assert_eq!(s.fields()[2], Value::new("việt"));
    }

    #[test]
    fn attribute_range_counts_characters() {
        let v = attr_list(vec![attribute(1, 1, 0, 4)]);
        assert_eq!(signature(&v), "(sa{sv}av)");
    }

    #[test]
    fn component_matches_libibus() {
        let v = component("/bin/x --ibus", "1.0", "pkey");
        assert_eq!(signature(&v), "(sa{sv}ssssssssavav)");
        let Value::Structure(s) = &v else { panic!() };
        let Value::Array(engines) = &s.fields()[11] else {
            panic!()
        };
        let Value::Value(desc) = &engines[0] else {
            panic!()
        };
        assert_eq!(signature(desc), "(sa{sv}ssssssssussssssss)");
    }

    #[test]
    fn array_elements_are_wrapped_once() {
        let props = registered_properties(vec![mode_property("VI", "t")]).to_string();
        assert!(props.contains("[<(\"IBusProperty\""), "{props}");
        let comp = component("/bin/x", "1", "pkey").to_string();
        assert!(comp.contains("[<(\"IBusEngineDesc\""), "{comp}");
        let t = text("ab", true).to_string();
        assert!(t.contains("[<(\"IBusAttribute\""), "{t}");
    }

    #[test]
    fn text_of_reads_back_what_text_wrote() {
        let t = Value::new(text("tiếng", false));
        assert_eq!(text_of(&t).as_deref(), Some("tiếng"));
        assert_eq!(text_of(&Value::from(3u32)), None);
    }

    #[test]
    fn property_signature() {
        let v = mode_property("VI", "Tiếng Việt");
        assert_eq!(signature(&v), "(sa{sv}suvsvbbuvv)");
    }
}
