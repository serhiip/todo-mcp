use crate::store::TodoStore;

#[tokio::test]
async fn add_and_get_all_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    let id1 = store.add("list1", "Title A".into(), "Body line one".into()).await.unwrap();
    let id2 = store.add("list1", "Title B".into(), "".into()).await.unwrap();
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    let items = store.get_all("list1").await.unwrap();
    assert_eq!(items.len(), 2);
    assert!(!items[0].completed);
    assert_eq!(items[0].id, 1);
    assert_eq!(items[0].title, "Title A");
    assert_eq!(items[0].body, "Body line one");
    assert_eq!(items[1].id, 2);
    assert_eq!(items[1].title, "Title B");
    assert_eq!(items[1].body, "");
}

#[tokio::test]
async fn body_indented_lines_not_parsed_as_items() {
    let dir = tempfile::tempdir().unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    store.add("list1", "Item".into(), "  - note\n  - [ ] nested".into()).await.unwrap();
    let items = store.get_all("list1").await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].body, "  - note\n  - [ ] nested");
}

#[tokio::test]
async fn complete_marks_item() {
    let dir = tempfile::tempdir().unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    let id1 = store.add("list1", "One".into(), "".into()).await.unwrap();
    store.add("list1", "Two".into(), "".into()).await.unwrap();
    let ok = store.complete("list1", id1).await.unwrap();
    assert!(ok);
    let items = store.get_all("list1").await.unwrap();
    assert!(items[0].completed);
    assert!(!items[1].completed);
    let ok2 = store.complete("list1", id1).await.unwrap();
    assert!(ok2);
    let ok3 = store.complete("list1", 99).await.unwrap();
    assert!(!ok3);
}

#[tokio::test]
async fn pick_returns_pending_only() {
    let dir = tempfile::tempdir().unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    let id1 = store.add("list1", "One".into(), "".into()).await.unwrap();
    store.complete("list1", id1).await.unwrap();
    store.add("list1", "Two".into(), "".into()).await.unwrap();
    let picked = store.pick("list1").await.unwrap();
    assert!(picked.is_some());
    assert_eq!(picked.unwrap().title, "Two");
}

#[tokio::test]
async fn pick_empty_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    let picked = store.pick("list1").await.unwrap();
    assert!(picked.is_none());
}

#[tokio::test]
async fn list_names_only_valid_stem_names() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("foo.md"), "- [ ] #1 x\n").unwrap();
    std::fs::write(dir.path().join("bar.md"), "- [ ] #1 y\n").unwrap();
    std::fs::write(dir.path().join("  foo  .md"), "- [ ] #1 z\n").unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    let names = store.list_names().await.unwrap();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"foo".to_string()));
    assert!(names.contains(&"bar".to_string()));
}

#[tokio::test]
async fn list_names_one_entry_per_sanitized_name() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("foo.md"), "- [ ] #1 x\n").unwrap();
    std::fs::write(dir.path().join("  foo  .md"), "- [ ] #1 y\n").unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    let names = store.list_names().await.unwrap();
    assert_eq!(names, ["foo"], "only canonical path adds name");
}

#[tokio::test]
async fn get_all_nonexistent_list_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    let items = store.get_all("missing").await.unwrap();
    assert!(items.is_empty());
}
