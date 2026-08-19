use std::{fmt::Write as _, fs, path::PathBuf};

const SCHEMA_PATH: &str = "../../packages/iyon-runtime/src/tui/bridge-schema.json";

const FIELDS: &[(&str, &str)] = &[
    ("schemaVersion", "VIEW_BRIDGE_SCHEMA_VERSION"),
    ("viewText", "VIEW_KIND_TEXT"),
    ("viewDiff", "VIEW_KIND_DIFF"),
    ("viewSpacer", "VIEW_KIND_SPACER"),
    ("viewRow", "VIEW_KIND_ROW"),
    ("viewColumn", "VIEW_KIND_COLUMN"),
    ("viewHanging", "VIEW_KIND_HANGING"),
    ("viewGrid", "VIEW_KIND_GRID"),
    ("viewContainer", "VIEW_KIND_CONTAINER"),
    ("viewClamp", "VIEW_KIND_CLAMP"),
    ("viewContentMax", "VIEW_KIND_CONTENT_MAX"),
    ("viewComponent", "VIEW_KIND_COMPONENT"),
    ("viewDecorated", "VIEW_KIND_DECORATED"),
    ("layoutNormal", "LAYOUT_CHILD_NORMAL"),
    ("layoutFixed", "LAYOUT_CHILD_FIXED"),
    ("layoutFlex", "LAYOUT_CHILD_FLEX"),
    ("layoutFlexMax", "LAYOUT_CHILD_FLEX_MAX"),
    ("layoutContentMax", "LAYOUT_CHILD_CONTENT_MAX"),
    ("trackContent", "GRID_TRACK_CONTENT"),
    ("trackContentMax", "GRID_TRACK_CONTENT_MAX"),
    ("trackFixed", "GRID_TRACK_FIXED"),
    ("trackFlex", "GRID_TRACK_FLEX"),
    ("trackFlexMax", "GRID_TRACK_FLEX_MAX"),
    ("overflowNone", "OVERFLOW_NONE"),
    ("overflowEllipsis", "OVERFLOW_ELLIPSIS"),
    ("overflowFooter", "OVERFLOW_FOOTER"),
    ("wrapWordThenGrapheme", "WRAP_WORD_THEN_GRAPHEME"),
    ("wrapGrapheme", "WRAP_GRAPHEME"),
    ("wrapNoWrap", "WRAP_NO_WRAP"),
    ("horizontalStart", "ALIGN_START"),
    ("horizontalCenter", "ALIGN_CENTER"),
    ("horizontalEnd", "ALIGN_END"),
    ("verticalTop", "VERTICAL_TOP"),
    ("verticalCenter", "VERTICAL_CENTER"),
    ("verticalBottom", "VERTICAL_BOTTOM"),
    ("diffContext", "DIFF_CONTEXT"),
    ("diffAddition", "DIFF_ADDITION"),
    ("diffDeletion", "DIFF_DELETION"),
    ("terminationTerminated", "DIFF_TERMINATED"),
    ("terminationUnterminated", "DIFF_UNTERMINATED"),
];

fn schema_number(source: &str, field: &str) -> u32 {
    let needle = format!("\"{field}\"");
    let occurrences = source.match_indices(&needle).count();
    if occurrences != 1 {
        panic!("bridge schema field {field} occurs {occurrences} times");
    }
    let start = source
        .find(&needle)
        .expect("bridge schema occurrence was counted");
    let value = source[start + needle.len()..]
        .split_once(':')
        .map(|(_, rest)| rest.trim_start())
        .and_then(|rest| {
            rest.split(|character: char| !character.is_ascii_digit())
                .next()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("bridge schema field {field} is not a number"));
    value
        .parse()
        .unwrap_or_else(|_| panic!("bridge schema field {field} does not fit u32"))
}

fn main() {
    napi_build::setup();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let schema_path = manifest_dir.join(SCHEMA_PATH);
    println!("cargo:rerun-if-changed={}", schema_path.display());
    let source = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("read bridge schema {}: {error}", schema_path.display()));

    let mut generated = String::new();
    for &(field, constant) in FIELDS {
        writeln!(
            generated,
            "pub const {constant}: u32 = {};",
            schema_number(&source, field)
        )
        .expect("writing bridge schema constants cannot fail");
    }
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set"))
        .join("tui_bridge_schema.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("write bridge schema {}: {error}", output.display()));
}
