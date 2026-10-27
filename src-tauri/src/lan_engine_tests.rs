use super::*;
#[test]
fn lan_resolves_only_current_enabled_local_provider_ids() {
    let root = tempfile::tempdir().unwrap();
    let library = tempfile::tempdir().unwrap();
    let photo = library.path().join("photo.png");
    fs::write(&photo, b"fixture").unwrap();
    let engine =
        Engine::new(Store::new(root.path().into(), root.path().join("cache")).unwrap()).unwrap();
    let source: Source = serde_json::from_value(serde_json::json!({"id":"local", "name":"local", "kind":"local", "location":library.path().to_string_lossy(), "enabled":true})).unwrap();
    engine
        .inner
        .lock()
        .unwrap()
        .state
        .sources
        .push(source.clone());
    let id = providers::item(
        photo.to_string_lossy().into(),
        Some("photo.png".into()),
        "local",
    )
    .id;
    assert_eq!(engine.lan_exports(&[id.clone()]).unwrap().len(), 1);
    assert!(engine
        .lan_exports(&[photo.to_string_lossy().into()])
        .is_err());
    assert!(engine.lan_exports(&["unknown".into()]).is_err());
    engine.inner.lock().unwrap().state.sources[0].enabled = false;
    assert!(engine.lan_exports(&[id]).is_err());
}
#[test]
fn lan_confirmation_preserves_fresh_runtime_settings_and_does_not_apply_wallpaper() {
    let root = tempfile::tempdir().unwrap();
    let engine =
        Engine::new(Store::new(root.path().into(), root.path().join("cache")).unwrap()).unwrap();
    let received = root.path().join("received");
    fs::create_dir(&received).unwrap();
    {
        let mut runtime = engine.inner.lock().unwrap();
        runtime.state.settings.interval_seconds = 4321;
        runtime.state.settings.order = "sequential".into();
        runtime.next_due = Instant::now() + Duration::from_secs(100);
    }
    let next_due = engine.inner.lock().unwrap().next_due;
    let state = engine.accept_lan_source(&received).unwrap();
    assert_eq!(state.settings.interval_seconds, 4321);
    assert_eq!(state.settings.order, "sequential");
    assert!(state.current.is_none());
    assert_eq!(state.sources.len(), 1);
    assert_eq!(engine.inner.lock().unwrap().next_due, next_due);
    assert_eq!(engine.store.load().unwrap().sources.len(), 1);
}
#[test]
fn lan_failed_persistence_does_not_mutate_runtime() {
    let root = tempfile::tempdir().unwrap();
    let engine =
        Engine::new(Store::new(root.path().into(), root.path().join("cache")).unwrap()).unwrap();
    fs::create_dir(root.path().join("state.json")).unwrap();
    assert!(engine.accept_lan_source(root.path()).is_err());
    assert!(engine.inner.lock().unwrap().state.sources.is_empty());
}
