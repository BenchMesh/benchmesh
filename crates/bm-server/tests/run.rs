//! Embedding API used by the desktop app: start, listen fallback, stop.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bm_server::run::{start, ServerOptions};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn get(addr: std::net::SocketAddr, path: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

#[test]
fn starts_falls_back_to_a_free_port_and_stops() {
    let dir = std::env::temp_dir().join(format!("bm-run-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Occupy a port so the first listen address fails.
    let blocker = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let taken = blocker.local_addr().unwrap().to_string();
    let server = start(ServerOptions {
        config: dir.join("config.yaml"), // missing: starts with no devices
        devices_dir: repo().join("devices"),
        ui_dir: None,
        listen: vec![taken.clone(), "127.0.0.1:0".into()],
        ws_interval: Duration::from_millis(200),
        recordings_db: None,
        simulate: None,
        data_dir: None,
        headless: None,
        extensions: Default::default(),
    })
    .unwrap();
    assert_ne!(server.addr.to_string(), taken, "fell back to the next address");
    assert!(server.url().starts_with("http://127.0.0.1:"));
    let resp = get(server.addr, "/status");
    assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
    assert!(resp.contains(r#""devices_total":0"#), "{resp}");
    let addr = server.addr;
    server.stop();
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err(), "listener closed after stop");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_definitions_dir_is_reported() {
    let err = start(ServerOptions {
        config: std::env::temp_dir().join("none.yaml"),
        devices_dir: PathBuf::from("/nonexistent/devices"),
        ui_dir: None,
        listen: vec!["127.0.0.1:0".into()],
        ws_interval: Duration::from_millis(200),
        recordings_db: None,
        simulate: None,
        data_dir: None,
        headless: None,
        extensions: Default::default(),
    })
    .err()
    .unwrap();
    assert!(err.contains("device definitions"), "{err}");
}

/// An extension of the distribution (`extension.rs`): its routes are served and documented, and
/// `/version` names the distribution.
#[test]
fn an_extension_adds_documented_routes_and_names_the_distribution() {
    use std::sync::Arc;

    use axum::routing::get as route_get;
    use bm_server::extension::{ApiRoute, Extension, Extensions};

    struct Hello;
    impl Extension for Hello {
        fn name(&self) -> &'static str {
            "hello"
        }
        fn routes(&self) -> axum::Router<bm_server::S> {
            axum::Router::new().route("/system/hello", route_get(|| async { "hello from an extension" }))
        }
        fn api(&self) -> Vec<ApiRoute> {
            vec![ApiRoute { method: "get", path: "/system/hello", tag: "system", summary: "Says hello", body: false, no_try: None }]
        }
        // background work with the server's state, on its runtime
        fn started(&self, state: &bm_server::S) {
            let state = state.clone();
            tokio::spawn(async move {
                state.community.reported("hello_def", Some("H1"));
                STARTED.store(true, std::sync::atomic::Ordering::SeqCst);
            });
        }
    }
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    let start_with = |extensions| {
        start(ServerOptions {
            config: std::env::temp_dir().join("none.yaml"),
            devices_dir: repo().join("devices"),
            ui_dir: None,
            listen: vec!["127.0.0.1:0".into()],
            ws_interval: Duration::from_millis(200),
            recordings_db: None,
            simulate: None,
            data_dir: None,
            headless: None,
            extensions,
        })
        .unwrap()
    };

    let official = start_with(Extensions { distribution: "official", list: vec![Arc::new(Hello)] });
    let hello = get(official.addr, "/system/hello");
    assert!(hello.starts_with("HTTP/1.1 200") && hello.ends_with("hello from an extension"), "{hello}");
    assert!(get(official.addr, "/openapi.json").contains(r#""summary":"Says hello""#));
    assert!(get(official.addr, "/version").contains(r#""distribution":"official""#));
    let t0 = std::time::Instant::now();
    while !STARTED.load(std::sync::atomic::Ordering::SeqCst) && t0.elapsed() < std::time::Duration::from_secs(5) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(STARTED.load(std::sync::atomic::Ordering::SeqCst), "the extension was told the server started");
    official.stop();

    let community = start_with(Extensions::default());
    assert!(get(community.addr, "/system/hello").starts_with("HTTP/1.1 404"));
    assert!(get(community.addr, "/version").contains(r#""distribution":"community""#));
    community.stop();
}
