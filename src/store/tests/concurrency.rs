use crate::store::TodoStore;

#[tokio::test]
async fn duplicate_id_complete_marks_first_match() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dup.md");
    std::fs::write(
        &path,
        "- [ ] #1 First\n- [ ] #1 Second\n- [ ] #2 Third\n",
    )
    .unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    let ok = store.complete("dup", 1).await.unwrap();
    assert!(ok);
    let items = store.get_all("dup").await.unwrap();
    let with_id1: Vec<_> = items.iter().filter(|i| i.id == 1).collect();
    assert_eq!(with_id1.len(), 2);
    assert!(with_id1[0].completed, "complete marks first occurrence");
    assert!(!with_id1[1].completed);
}

#[tokio::test]
async fn duplicate_id_pick_returns_one_pending() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("dup2.md"),
        "- [ ] #1 A\n- [ ] #1 B\n",
    )
    .unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    let picked = store.pick("dup2").await.unwrap();
    assert!(picked.is_some());
    assert_eq!(picked.unwrap().id, 1);
}

#[tokio::test]
async fn out_of_order_ids_complete_and_pick_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("out.md"),
        "- [ ] #5 X\n- [ ] #2 Y\n- [ ] #8 Z\n",
    )
    .unwrap();
    let store = TodoStore::new(dir.path().to_path_buf());
    let ok = store.complete("out", 5).await.unwrap();
    assert!(ok);
    let items = store.get_all("out").await.unwrap();
    let id5 = items.iter().find(|i| i.id == 5).unwrap();
    assert!(id5.completed);
    let picked = store.pick("out").await.unwrap();
    assert!(picked.is_some());
    assert!([2, 8].contains(&picked.unwrap().id));
}
