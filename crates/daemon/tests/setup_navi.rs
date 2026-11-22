use serde_json::Value;
use wobblescope::setup::navi::{Merge, Strip, entry, new_file, with_entry, without_entry};

fn merged(text: &str) -> String {
    match with_entry(text).unwrap() {
        Merge::Changed(text) => text,
        Merge::Unchanged => panic!("expected a change"),
    }
}

fn items(text: &str) -> Vec<Value> {
    serde_json::from_str(text).unwrap()
}

#[test]
fn an_entry_is_added_after_the_last_one_with_the_files_own_indentation() {
    let text =
        "[\n    {\n        \"title\": \"Docs\",\n        \"href\": \"https://x.test\"\n    }\n]\n";
    let out = merged(text);
    assert!(out.starts_with(&text[..text.len() - 3]), "{out}");
    assert!(out.ends_with("    },\n    {\n        \"title\": \"Wobblescope\",\n        \"href\": \"/wobblescope/\",\n        \"target\": \"_self\",\n        \"position\": 95,\n        \"icon\": \"M9.96,11.31C10.82,8.1 11.5,6 13,6C14.5,6 15.18,8.1 16.04,11.31C17,14.92 18.1,19 22,19V17C19.8,17 19,14.54 17.97,10.8C17.08,7.46 16.15,4 13,4C9.85,4 8.92,7.46 8.03,10.8C7.03,14.54 6.2,17 4,17V2H2V22H22V20H4V19C7.9,19 9,14.92 9.96,11.31Z\"\n    }\n]\n"), "{out}");
    assert_eq!(items(&out).len(), 2);
    assert_eq!(items(&out)[1], entry());
}

#[test]
fn a_tab_indented_or_one_line_array_keeps_its_style() {
    let tabs = merged("[\n\t{\"title\": \"A\"}\n]");
    assert!(
        tabs.starts_with(
            "[\n\t{\"title\": \"A\"},\n\t{\"title\": \"Wobblescope\", \"href\": \"/wobblescope/\""
        ),
        "{tabs}"
    );
    assert!(tabs.ends_with("}\n]"));
    let line = merged("[{\"title\":\"A\"}]");
    assert!(
        line.starts_with("[{\"title\":\"A\"}, {\"title\": \"Wobblescope\""),
        "{line}"
    );
    assert_eq!(items(&line).len(), 2);
}

#[test]
fn an_equal_entry_is_left_and_a_different_one_is_replaced_in_place() {
    let text = format!(
        "[\n  {{\"title\": \"A\"}},\n{},\n  {{\"title\": \"B\"}}\n]\n",
        "  {\"title\": \"Wobblescope\", \"href\": \"/wobblescope/\", \"position\": 99}"
    );
    let out = merged(&text);
    let parsed = items(&out);
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[1], entry());
    assert!(out.starts_with("[\n  {\"title\": \"A\"},\n  {\"title\": \"Wobblescope\""));
    assert!(out.ends_with("},\n  {\"title\": \"B\"}\n]\n"));
    assert_eq!(with_entry(&out), Ok(Merge::Unchanged));
}

#[test]
fn a_file_that_is_not_an_array_is_refused() {
    for text in ["{\"title\": \"x\"}", "not json", "", "[1, 2"] {
        assert!(with_entry(text).is_err(), "{text:?}");
    }
}

#[test]
fn an_empty_array_becomes_a_new_pretty_file() {
    assert_eq!(with_entry("[]"), Ok(Merge::Changed(new_file())));
    assert_eq!(items(&new_file()), [entry()]);
    assert!(new_file().starts_with("[\n  {\n    \"title\": \"Wobblescope\","));
}

#[test]
fn removal_drops_only_our_entry() {
    let text = merged("[{\"title\": \"A\"}]");
    let Strip::Removed { text: out, empty } = without_entry(&text).unwrap() else {
        panic!("expected removal");
    };
    assert!(!empty);
    assert_eq!(items(&out), items("[{\"title\": \"A\"}]"));
    let Strip::Removed { empty, .. } = without_entry(&new_file()).unwrap() else {
        panic!("expected removal");
    };
    assert!(empty);
    assert_eq!(without_entry("[{\"title\": \"A\"}]"), Ok(Strip::Absent));
    assert!(without_entry("{}").is_err());
}
