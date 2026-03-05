use crate::store::{format_item, parse_content, TodoStore};

#[test]
fn parse_format_blank_body_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let _store = TodoStore::new(dir.path().to_path_buf());
    let input = "- [ ] #1 Title only\n\n- [x] #2 Done\n";
    let items = parse_content(input);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, 1);
    assert_eq!(items[0].title, "Title only");
    assert_eq!(items[0].body, "");
    assert_eq!(items[1].body, "");
    let formatted: String = items.iter().map(format_item).collect::<Vec<_>>().join("\n");
    let round = parse_content(&formatted);
    assert_eq!(round.len(), items.len());
    assert_eq!(round[0].id, items[0].id);
    assert_eq!(round[0].body, "");
}

#[test]
fn parse_legacy_dash_item_assigns_stable_id() {
    let input = "- Item without hash\n- [ ] #2 With id\n";
    let items = parse_content(input);
    assert_eq!(items.len(), 2);
    assert!(items[0].id >= 1);
    assert_eq!(items[0].title, "Item without hash");
    assert_eq!(items[1].id, 2);
    let formatted: String = items.iter().map(format_item).collect::<Vec<_>>().join("\n");
    let round = parse_content(&formatted);
    assert_eq!(round[0].id, items[0].id);
    assert_eq!(round[1].id, 2);
}

#[test]
fn parse_body_unusual_indentation_preserved_after_roundtrip() {
    let input = "- [ ] #1 Head\n    four spaces\n   three\n  two\n";
    let items = parse_content(input);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].body, "  four spaces\n three\ntwo");
    let formatted: String = items.iter().map(format_item).collect::<Vec<_>>().join("\n");
    let round = parse_content(&formatted);
    assert_eq!(round[0].body, items[0].body);
}

#[test]
fn parse_multiple_legacy_items_stable_ids() {
    let input = "- First\n- Second\n- [x] #10 Third\n";
    let items = parse_content(input);
    assert_eq!(items.len(), 3);
    let ids: Vec<u32> = items.iter().map(|i| i.id).collect();
    assert!(ids[0] >= 1 && ids[1] > ids[0] && ids[2] == 10);
    let formatted: String = items.iter().map(format_item).collect::<Vec<_>>().join("\n");
    let round = parse_content(&formatted);
    for (a, b) in round.iter().zip(items.iter()) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.title.trim(), b.title.trim());
    }
}

#[test]
fn parse_empty_body_lines_do_not_break_items() {
    let input = "- [ ] #1 A\n\n\n- [ ] #2 B\n";
    let items = parse_content(input);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].body, "");
    assert_eq!(items[1].body, "");
}

#[test]
fn parse_malformed_markdown_no_panic() {
    let cases = [
        "- [ ] #",
        "- [x] #abc",
        "- [  ] #1 x",
        "- [ ] #1\n  - [",
        "- ",
        "",
    ];
    for input in cases {
        let _ = parse_content(input);
    }
}

#[test]
fn parse_mixed_legacy_and_new_id_headers() {
    let input = "- legacy only\n- [ ] checkbox no id\n- [ ] #3 With id\n- [x] #4 Done\n";
    let items = parse_content(input);
    assert_eq!(items.len(), 4);
    assert!(items[0].id >= 1);
    assert_eq!(items[0].title, "legacy only");
    assert!(items[1].id >= 1);
    assert_eq!(items[1].title, "checkbox no id");
    assert_eq!(items[2].id, 3);
    assert_eq!(items[3].id, 4);
    assert!(items[3].completed);
    let formatted: String = items.iter().map(format_item).collect::<Vec<_>>().join("\n");
    let round = parse_content(&formatted);
    assert_eq!(round.len(), 4);
}

#[test]
fn parse_truncated_markdown_no_panic() {
    let input = "- [ ] #1 Incomplete\n  body line\n- [ ] #2";
    let items = parse_content(input);
    assert!(!items.is_empty());
}
