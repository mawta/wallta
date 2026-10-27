use wallta_lib::{
    models::{AppState, Settings},
    providers::{parse_manifest, parse_webdav},
    rotation::{choose_index, schedule_allows},
    storage::public_state,
};

#[test]
fn shuffle_consumes_a_whole_cycle_and_survives_restart() {
    use wallta_lib::rotation::ShuffleBag;
    let ids = vec!["a".into(), "b".into(), "c".into()];
    let mut bag = ShuffleBag::default();
    let first = bag.next(&ids, None).unwrap();
    let encoded = serde_json::to_string(&bag).unwrap();
    let mut bag: ShuffleBag = serde_json::from_str(&encoded).unwrap();
    let second = bag.next(&ids, Some(&first)).unwrap();
    let third = bag.next(&ids, Some(&second)).unwrap();
    assert_ne!(first, second);
    assert_ne!(first, third);
    assert_ne!(second, third);
    assert_ne!(bag.next(&ids, Some(&third)).unwrap(), third);
}

#[test]
fn shuffle_reconciles_removed_and_new_catalog_ids() {
    use wallta_lib::rotation::ShuffleBag;
    let mut bag = ShuffleBag {
        known: vec!["a".into(), "b".into()],
        remaining: vec!["b".into()],
    };
    let ids = vec!["a".into(), "c".into()];
    assert_eq!(bag.next(&ids, Some("a")), Some("c".into()));
    assert!(!bag.known.contains(&"b".to_string()));
}

#[test]
fn offline_catalog_requires_same_source_and_existing_download() {
    use wallta_lib::providers::item;
    use wallta_lib::storage::{offline_items, source_fingerprint, Catalog};
    let mut source: wallta_lib::models::Source = serde_json::from_str(r#"{"id":"a","name":"Album","kind":"manifest","location":"https://example.com/list.json","enabled":true,"recursive":false}"#).unwrap();
    let image = item("https://example.com/one.jpg".into(), None, &source.id);
    let cache = tempfile::tempdir().unwrap();
    let catalog = Catalog {
        fingerprint: source_fingerprint(&source),
        items: vec![image.clone()],
    };
    assert!(offline_items(&source, Some(&catalog), cache.path()).is_empty());
    std::fs::write(
        cache.path().join(format!("{}.img", image.id)),
        b"downloaded",
    )
    .unwrap();
    assert_eq!(
        offline_items(&source, Some(&catalog), cache.path()).len(),
        1
    );
    source.location = "https://example.com/other.json".into();
    assert!(offline_items(&source, Some(&catalog), cache.path()).is_empty());
    source.location = "https://example.com/list.json".into();
    source.access_token = Some("changed".into());
    assert!(offline_items(&source, Some(&catalog), cache.path()).is_empty());
    source.access_token = None;
    source.enabled = false;
    assert!(offline_items(&source, Some(&catalog), cache.path()).is_empty());
    source.enabled = true;
    source.kind = "photos".into();
    let photos = Catalog {
        fingerprint: source_fingerprint(&source),
        items: vec![image],
    };
    assert!(offline_items(&source, Some(&photos), cache.path()).is_empty());
}

#[test]
fn drive_recursive_walk_follows_folders_once_and_skips_nonimages() {
    use wallta_lib::providers::walk_drive;
    let source: wallta_lib::models::Source = serde_json::from_str(r#"{"id":"a","name":"Drive","kind":"drive","location":"root","enabled":true,"recursive":true}"#).unwrap();
    let mut visits = Vec::new();
    let items = walk_drive(&source, "root", |folder, _| {
        visits.push(folder.to_string());
        Ok(if folder == "root" { serde_json::json!({"files":[{"id":"sub","name":"Sub","mimeType":"application/vnd.google-apps.folder"},{"id":"one","name":"One","mimeType":"image/jpeg"},{"id":"doc","name":"Doc","mimeType":"text/plain"}]}) }
        else { serde_json::json!({"files":[{"id":"root","name":"Cycle","mimeType":"application/vnd.google-apps.folder"},{"id":"two","name":"Two","mimeType":"image/png"}]}) })
    }).unwrap();
    assert_eq!(visits, vec!["root", "sub"]);
    assert_eq!(items.len(), 2);
}

#[test]
fn drive_nonrecursive_walk_pages_only_the_selected_folder() {
    use wallta_lib::providers::walk_drive;
    let source: wallta_lib::models::Source = serde_json::from_str(r#"{"id":"a","name":"Drive","kind":"drive","location":"root","enabled":true,"recursive":false}"#).unwrap();
    let mut pages = Vec::new();
    let items = walk_drive(&source, "root", |folder, token| {
        assert_eq!(folder, "root"); pages.push(token.to_string());
        Ok(if token.is_empty() { serde_json::json!({"nextPageToken":"second","files":[{"id":"sub","name":"Sub","mimeType":"application/vnd.google-apps.folder"}]}) }
        else { serde_json::json!({"files":[{"id":"one","name":"One","mimeType":"image/jpeg"}]}) })
    }).unwrap();
    assert_eq!(pages, vec!["", "second"]);
    assert_eq!(items.len(), 1);
}

#[test]
fn source_disconnect_removes_catalog_and_downloaded_previews() {
    use wallta_lib::providers::item;
    use wallta_lib::storage::{reconcile_catalogs, source_fingerprint, Catalog, RuntimeState};
    let source: wallta_lib::models::Source = serde_json::from_str(r#"{"id":"a","name":"Album","kind":"manifest","location":"https://example.com/list.json","enabled":true,"recursive":false}"#).unwrap();
    let media = item("https://example.com/a.jpg".into(), None, &source.id);
    let cache = tempfile::tempdir().unwrap();
    let original = cache.path().join(format!("{}.img", media.id));
    let preview = cache.path().join(format!("{}-preview.png", media.id));
    std::fs::write(&original, b"image").unwrap();
    std::fs::write(&preview, b"preview").unwrap();
    let mut runtime = RuntimeState::default();
    runtime.catalogs.insert(
        source.id.clone(),
        Catalog {
            fingerprint: source_fingerprint(&source),
            items: vec![media],
        },
    );
    reconcile_catalogs(&mut runtime, &[], cache.path());
    assert!(runtime.catalogs.is_empty());
    assert!(!original.exists());
    assert!(!preview.exists());
}

#[test]
fn overnight_window_belongs_to_day_it_started() {
    let mut s = Settings::default();
    s.schedule_enabled = true;
    s.days = vec![1];
    s.start_time = "22:00".into();
    s.end_time = "06:00".into();
    assert!(schedule_allows(&s, 1, 23 * 60));
    assert!(schedule_allows(&s, 2, 5 * 60));
    assert!(!schedule_allows(&s, 1, 5 * 60));
    assert!(!schedule_allows(&s, 2, 7 * 60));
}
#[test]
fn shuffle_excludes_previous_image() {
    for _ in 0..200 {
        assert_ne!(choose_index(4, Some(2), true), Some(2));
    }
    assert_eq!(choose_index(1, Some(0), true), Some(0));
    assert_eq!(choose_index(0, None, true), None);
    assert_eq!(choose_index(4, Some(3), false), Some(0));
}
#[test]
fn manifest_resolves_relative_urls_and_names() {
    let items = parse_manifest(
        r#"{"images":["one.jpg",{"url":"/two.png","name":"Two"}]}"#,
        "https://example.com/album/list.json",
        "s",
    )
    .unwrap();
    assert_eq!(items[0].url, "https://example.com/album/one.jpg");
    assert_eq!(items[1].name, "Two");
    assert!(parse_manifest(
        r#"["file:///C:/secret.png"]"#,
        "https://example.com/list",
        "s"
    )
    .is_err());
}
#[test]
fn webdav_rejects_cross_origin_and_collection_entries() {
    let xml = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/photos/a%20b.jpg</d:href><d:propstat><d:prop><d:getcontenttype>image/jpeg</d:getcontenttype></d:prop></d:propstat></d:response><d:response><d:href>https://evil.test/a.jpg</d:href></d:response><d:response><d:href>/photos/sub/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response></d:multistatus>"#;
    let items = parse_webdav(xml, "https://dav.test/photos/", "s").unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].url, "https://dav.test/photos/a%20b.jpg");
}
#[test]
fn persisted_state_never_contains_credentials() {
    let mut state: AppState = serde_json::from_str(r#"{"version":1,"sources":[{"id":"a","name":"DAV","kind":"webdav","location":"https://dav.test/","recursive":false,"enabled":true,"password":"secret","accessToken":"token","apiKey":"key"}],"settings":{},"running":false,"current":null,"lastChanged":null,"error":null}"#).unwrap();
    let clean = public_state(&mut state);
    let json = serde_json::to_string(&clean).unwrap();
    assert!(!json.contains("secret"));
    assert!(!json.contains("token"));
    assert!(!json.contains("\"key\""));
}

#[test]
fn disconnected_photos_cache_is_erased_without_a_persisted_catalog() {
    use wallta_lib::storage::{reconcile_catalogs, RuntimeState};
    let cache = tempfile::tempdir().unwrap();
    let id = "a".repeat(64);
    for suffix in [".img", "-preview.png"] {
        std::fs::write(cache.path().join(format!("{id}{suffix}")), b"cached").unwrap();
    }
    let mut runtime: RuntimeState = serde_json::from_value(serde_json::json!({
        "cacheOwners": {id.clone(): {"sourceId":"photos", "fingerprint":"old"}}
    }))
    .unwrap();
    reconcile_catalogs(&mut runtime, &[], cache.path());
    assert_eq!(std::fs::read_dir(cache.path()).unwrap().count(), 0);
}

#[test]
fn disconnected_source_erases_downloads_absent_from_latest_listing() {
    use wallta_lib::storage::{reconcile_catalogs, source_fingerprint, RuntimeState};
    let source: wallta_lib::models::Source = serde_json::from_str(r#"{"id":"a","name":"Album","kind":"manifest","location":"https://example.com/list","enabled":true}"#).unwrap();
    let cache = tempfile::tempdir().unwrap();
    let id = "b".repeat(64);
    std::fs::write(cache.path().join(format!("{id}.img")), b"cached").unwrap();
    let mut runtime: RuntimeState = serde_json::from_value(serde_json::json!({
        "catalogs": {"a": {"fingerprint":source_fingerprint(&source),"items":[]}},
        "cacheOwners": {id.clone(): {"sourceId":"a", "fingerprint":source_fingerprint(&source)}}
    }))
    .unwrap();
    reconcile_catalogs(&mut runtime, &[source.clone()], cache.path());
    assert!(cache.path().join(format!("{id}.img")).exists());
    reconcile_catalogs(&mut runtime, &[], cache.path());
    assert!(!cache.path().join(format!("{id}.img")).exists());
}
