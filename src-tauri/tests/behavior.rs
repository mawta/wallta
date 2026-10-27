use wallta_lib::{models::{AppState, Settings}, providers::{parse_manifest, parse_webdav}, rotation::{schedule_allows, choose_index}, storage::public_state};

#[test]
fn overnight_window_belongs_to_day_it_started() {
    let mut s = Settings::default();
    s.schedule_enabled = true; s.days = vec![1]; s.start_time = "22:00".into(); s.end_time = "06:00".into();
    assert!(schedule_allows(&s, 1, 23 * 60));
    assert!(schedule_allows(&s, 2, 5 * 60));
    assert!(!schedule_allows(&s, 1, 5 * 60));
    assert!(!schedule_allows(&s, 2, 7 * 60));
}
#[test]
fn shuffle_excludes_previous_image() {
    for _ in 0..200 { assert_ne!(choose_index(4, Some(2), true), Some(2)); }
    assert_eq!(choose_index(1, Some(0), true), Some(0));
    assert_eq!(choose_index(0, None, true), None);
    assert_eq!(choose_index(4, Some(3), false), Some(0));
}
#[test]
fn manifest_resolves_relative_urls_and_names() {
    let items = parse_manifest(r#"{"images":["one.jpg",{"url":"/two.png","name":"Two"}]}"#, "https://example.com/album/list.json", "s").unwrap();
    assert_eq!(items[0].url, "https://example.com/album/one.jpg");
    assert_eq!(items[1].name, "Two");
    assert!(parse_manifest(r#"["file:///C:/secret.png"]"#, "https://example.com/list", "s").is_err());
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
    assert!(!json.contains("secret")); assert!(!json.contains("token")); assert!(!json.contains("\"key\""));
}
