use super::*;
use std::io::{Cursor, Read, Write};

fn png() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
fn fixture(
    ttl: Duration,
    outgoing: Vec<ExportInput>,
) -> (tempfile::TempDir, LanManager, LanStatus) {
    let root = tempfile::tempdir().unwrap();
    let manager = LanManager::new(root.path()).unwrap();
    let status = manager
        .start_on(outgoing, vec![Ipv4Addr::LOCALHOST], ttl)
        .unwrap();
    (root, manager, status)
}
fn request(url: &str, method: &str, route: &str, extra: &str, body: &[u8]) -> Vec<u8> {
    let url = url::Url::parse(url).unwrap();
    let host = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap());
    let mut stream = TcpStream::connect(&host).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    write!(
        stream,
        "{method} {}{route} HTTP/1.1\r\nHost: {host}\r\n{extra}\r\n",
        url.path()
    )
    .unwrap();
    stream.write_all(body).unwrap();
    let mut response = Vec::new();
    let read = stream.read_to_end(&mut response);
    assert!(
        read.is_ok() || !response.is_empty(),
        "No HTTP response: {read:?}"
    );
    response
}
fn upload(url: &str, bytes: &[u8]) -> Vec<u8> {
    let parsed = url::Url::parse(url).unwrap();
    request(
        url,
        "POST",
        "upload?name=..%2Funsafe%0D%0A.png",
        &format!(
            "Origin: {}\r\nContent-Length: {}\r\n",
            parsed.origin().ascii_serialization(),
            bytes.len()
        ),
        bytes,
    )
}
fn code(response: &[u8]) -> u16 {
    std::str::from_utf8(&response[..12])
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}
#[test]
fn tcp_upload_requires_confirmation_and_selected_copy_downloads() {
    let exports = tempfile::tempdir().unwrap();
    let original = exports.path().join("chosen.png");
    fs::write(&original, png()).unwrap();
    let (_root, manager, status) = fixture(
        Duration::from_secs(60),
        vec![ExportInput {
            id: "chosen".into(),
            name: "bad\r\n\".png".into(),
            path: original.clone(),
        }],
    );
    assert_eq!(status.outgoing.len(), 1);
    fs::remove_file(original).unwrap(); // Session serves its bounded snapshot only.
    let downloaded = request(&status.urls[0], "GET", "files/chosen", "", &[]);
    assert_eq!(code(&downloaded), 200);
    assert!(downloaded.ends_with(&png()));
    assert_eq!(
        code(&request(
            &status.urls[0],
            "GET",
            "files/../../secret",
            "",
            &[]
        )),
        404
    );
    assert_eq!(code(&upload(&status.urls[0], &png())), 200);
    assert_eq!(manager.status().received_count, 1);
    assert!(!manager.root.join("received").exists());
    let saved = manager
        .accept_with(|path| {
            assert_eq!(fs::read_dir(path).unwrap().count(), 1);
            Ok(path.to_owned())
        })
        .unwrap();
    assert!(saved.is_dir());
    assert!(!manager.status().active);
    manager.stop();
    assert!(saved.is_dir()); // Stop cannot remove an accepted source.
}
#[test]
fn tcp_rejects_token_host_origin_and_ambiguous_framing() {
    let (_root, manager, status) = fixture(Duration::from_secs(60), vec![]);
    let url = &status.urls[0];
    let wrong = url.replace(
        url::Url::parse(url).unwrap().path(),
        &format!("/{}/", "0".repeat(64)),
    );
    assert_eq!(code(&request(&wrong, "GET", "", "", &[])), 404);
    for headers in [
        "Host: attacker.test\r\n",
        "Origin: http://evil.test\r\nContent-Length: 1\r\n",
        "Content-Length: 1\r\n",
        "Origin: null\r\nContent-Length: 1\r\n",
        "Origin: http://evil.test\r\nContent-Length: 1\r\nContent-Length: 1\r\n",
        "Transfer-Encoding: chunked\r\nContent-Length: 1\r\n",
    ] {
        assert_ne!(code(&request(url, "POST", "upload", headers, b"x")), 200);
    }
    let origin = url::Url::parse(url).unwrap().origin().ascii_serialization();
    for framing in [
        "Content-Length: 1\r\nContent-Length: 1\r\n",
        "Content-Length: 1, 1\r\n",
        "Transfer-Encoding: chunked\r\nContent-Length: 1\r\n",
    ] {
        assert_eq!(
            code(&request(
                url,
                "POST",
                "upload",
                &format!("Origin: {origin}\r\n{framing}"),
                b"x"
            )),
            400
        );
    }
    assert_eq!(manager.status().received_count, 0);
}
#[test]
fn tcp_rejects_nonimage_oversize_and_upload_count_limit() {
    let (_root, manager, status) = fixture(Duration::from_secs(60), vec![]);
    let url = &status.urls[0];
    assert_eq!(code(&upload(url, b"not an image")), 415);
    let origin = url::Url::parse(url).unwrap().origin().ascii_serialization();
    assert_eq!(
        code(&request(
            url,
            "POST",
            "upload",
            &format!(
                "Origin: {origin}\r\nContent-Length: {}\r\n",
                MAX_FILE_BYTES + 1
            ),
            &[]
        )),
        413
    );
    for _ in 0..MAX_FILES {
        assert_eq!(code(&upload(url, &png())), 200);
    }
    assert_eq!(code(&upload(url, &png())), 413);
    assert_eq!(manager.status().received_count, MAX_FILES);
}
#[test]
fn stop_closes_partial_upload_and_erases_staging() {
    let (_root, manager, status) = fixture(Duration::from_secs(60), vec![]);
    assert_eq!(code(&upload(&status.urls[0], &png())), 200);
    let parsed = url::Url::parse(&status.urls[0]).unwrap();
    let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, parsed.port().unwrap())).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(socket, "POST {}upload HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: http://127.0.0.1:{}\r\nContent-Length: 1000\r\n\r\nx", parsed.path(), parsed.port().unwrap(), parsed.port().unwrap()).unwrap();
    std::thread::sleep(Duration::from_millis(40));
    let began = Instant::now();
    manager.stop();
    assert!(began.elapsed() < Duration::from_secs(2));
    match socket.read(&mut [0; 1]) {
        Ok(0) => (),
        Ok(_) => panic!("Expiry must close the socket, not send an early HTTP error"),
        Err(error) => assert!(matches!(
            error.kind(),
            std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
        )),
    }
    assert_eq!(
        fs::read_dir(manager.root.join("staging")).unwrap().count(),
        0
    );
    assert_eq!(manager.status().expires_at, 0);
}
#[test]
fn expiry_and_failed_accept_clean_unconfirmed_files_only() {
    let (root, manager, status) = fixture(Duration::from_millis(100), vec![]);
    assert_eq!(code(&upload(&status.urls[0], &png())), 200);
    std::thread::sleep(Duration::from_millis(180));
    assert!(!manager.status().active);
    assert_eq!(
        fs::read_dir(manager.root.join("staging")).unwrap().count(),
        0
    );
    let status = manager
        .start_on(vec![], vec![Ipv4Addr::LOCALHOST], Duration::from_secs(60))
        .unwrap();
    assert_eq!(code(&upload(&status.urls[0], &png())), 200);
    assert!(manager
        .accept_with::<()>(|_| Err("cannot save".into()))
        .is_err());
    assert_eq!(
        fs::read_dir(manager.root.join("received")).unwrap().count(),
        0
    );
    fs::write(root.path().join("unrelated"), b"keep").unwrap();
    drop(manager);
    let _ = LanManager::new(root.path()).unwrap();
    assert!(root.path().join("unrelated").is_file());
}

#[test]
fn public_status_is_camel_case_and_never_exposes_paths() {
    let (_root, manager, status) = fixture(Duration::from_secs(60), vec![]);
    let response = request(&status.urls[0], "GET", "status", "", &[]);
    assert_eq!(code(&response), 200);
    let body = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let value: serde_json::Value = serde_json::from_slice(&response[body..]).unwrap();
    assert!(value["active"].as_bool().unwrap());
    assert!(value["expiresAt"].as_u64().unwrap() > epoch_ms());
    assert_eq!(value["receivedCount"], 0);
    assert!(value.get("root").is_none());
    let token = url::Url::parse(&status.urls[0]).unwrap().path()[1..65].to_owned();
    assert!(valid_token(&token));
    manager.stop();
    let inactive = manager.status();
    assert!(inactive.urls.is_empty());
    assert_eq!(inactive.expires_at, 0);
}
#[test]
fn total_limit_and_pixel_bomb_are_rejected_before_staging() {
    let (_root, manager, status) = fixture(Duration::from_secs(60), vec![]);
    {
        let session = manager.session.lock().unwrap();
        session
            .as_ref()
            .unwrap()
            .shared
            .received
            .lock()
            .unwrap()
            .bytes = MAX_TOTAL_BYTES - 1;
    }
    assert_eq!(code(&upload(&status.urls[0], &png())), 413);
    assert_eq!(manager.status().received_count, 0);
    let mut bomb = png();
    bomb[16..20].copy_from_slice(&(120_000_001u32).to_be_bytes());
    bomb[20..24].copy_from_slice(&1u32.to_be_bytes());
    let mut crc = !0u32;
    for byte in &bomb[12..29] {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    bomb[29..33].copy_from_slice(&(!crc).to_be_bytes());
    assert_eq!(validate_image(&bomb), Err(413));
}
#[test]
fn missing_lan_address_and_stale_cleanup_are_scoped() {
    let root = tempfile::tempdir().unwrap();
    let staging = root.path().join("lan-transfer/staging");
    fs::create_dir_all(staging.join("f".repeat(64))).unwrap();
    fs::create_dir_all(staging.join("other-feature")).unwrap();
    let manager = LanManager::new(root.path()).unwrap();
    assert!(!staging.join("f".repeat(64)).exists());
    assert!(staging.join("other-feature").is_dir());
    assert!(manager
        .start_on(vec![], vec![], SESSION_TTL)
        .unwrap_err()
        .contains("No private"));
    assert!(!manager.status().active);
}
#[test]
fn tcp_foreign_host_is_rejected_even_with_valid_token() {
    let (_root, _manager, status) = fixture(Duration::from_secs(60), vec![]);
    let url = url::Url::parse(&status.urls[0]).unwrap();
    let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, url.port().unwrap())).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        socket,
        "GET {} HTTP/1.1\r\nHost: attacker.test:{}\r\n\r\n",
        url.path(),
        url.port().unwrap()
    )
    .unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    assert_eq!(code(&response), 403);
}

#[test]
fn tcp_jpeg_webp_and_png_validate_actual_contents() {
    let (_root, manager, status) = fixture(Duration::from_secs(60), vec![]);
    for format in [
        image::ImageFormat::Jpeg,
        image::ImageFormat::WebP,
        image::ImageFormat::Png,
    ] {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut bytes, format)
            .unwrap();
        assert_eq!(code(&upload(&status.urls[0], &bytes.into_inner())), 200);
    }
    assert_eq!(manager.status().received_count, 3);
    assert_eq!(code(&upload(&status.urls[0], b"GIF89a unsupported")), 415);
}
#[test]
fn expiry_closes_partial_socket_without_native_polling() {
    let (_root, manager, status) = fixture(Duration::from_millis(100), vec![]);
    let url = url::Url::parse(&status.urls[0]).unwrap();
    let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, url.port().unwrap())).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(socket, "POST {}upload HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: http://127.0.0.1:{}\r\nContent-Length: 1000\r\n\r\nx", url.path(), url.port().unwrap(), url.port().unwrap()).unwrap();
    let began = Instant::now();
    match socket.read(&mut [0; 1]) {
        Ok(_) => (),
        Err(error) => assert!(matches!(
            error.kind(),
            std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
        )),
    }
    assert!(began.elapsed() < Duration::from_secs(2));
    // Wait only for the worker's final cleanup instruction, without calling status/stop.
    let cleanup_deadline = Instant::now() + Duration::from_secs(2);
    while fs::read_dir(manager.root.join("staging")).unwrap().count() != 0
        && Instant::now() < cleanup_deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fs::read_dir(manager.root.join("staging")).unwrap().count(),
        0
    );
}

#[test]
fn tcp_upload_waits_for_delayed_body_packets() {
    let (_root, manager, status) = fixture(Duration::from_secs(60), vec![]);
    let url = url::Url::parse(&status.urls[0]).unwrap();
    let port = url.port().unwrap();
    let mut socket = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let bytes = png();
    write!(socket, "POST {}upload HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Length: {}\r\n\r\n", url.path(), bytes.len()).unwrap();
    socket.write_all(&bytes[..1]).unwrap();
    std::thread::sleep(Duration::from_millis(75));
    socket.write_all(&bytes[1..]).unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    assert_eq!(code(&response), 200);
    assert_eq!(manager.status().received_count, 1);
}
