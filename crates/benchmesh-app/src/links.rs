//! Web pages the UI opens in the system browser (the webview does not follow external links).
//! A fixed list: the UI is served over HTTP, so it names a page and never passes a URL.

pub const RELEASES: &str = "https://github.com/BenchMesh/benchmesh/releases/latest";
const PAGES: [(&str, &str); 12] = [
    ("issues", "https://github.com/BenchMesh/benchmesh/issues"),
    ("new-issue", "https://github.com/BenchMesh/benchmesh/issues/new"),
    ("releases", RELEASES),
    ("docs", "https://benchmesh.online/docs"),
    ("docs-scpi", "https://benchmesh.online/docs/scpi"),
    ("docs-python", "https://benchmesh.online/docs/python"),
    ("headless", "https://benchmesh.online/docs/headless"),
    ("linux-usb", bm_server::access_help::USB_HELP),
    ("linux-serial", bm_server::access_help::SERIAL_HELP),
    ("macos-local-network", bm_server::access_help::LOCAL_NETWORK_HELP),
    // System Settings → Privacy & Security → Local Network (`open` hands it to System Settings)
    ("macos-network-settings", "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork"),
    ("email", "mailto:support@benchmesh.online"),
];

pub fn url_of(page: &str) -> Option<&'static str> {
    PAGES.iter().find(|(name, _)| *name == page).map(|(_, url)| *url)
}

/// Open `url` with the system's default browser.
pub fn open(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    let child = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let child = std::process::Command::new("open").arg(url).spawn();
    #[cfg(windows)]
    let child = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    child.map(|_| ())
}

/// Open one of the known pages (`issues`, `new-issue`, `releases`, `docs`, `docs-scpi`, `docs-python`, `headless`, `linux-usb`, `linux-serial`, `macos-local-network`, `macos-network-settings`, `email`: the mail app).
#[tauri::command]
pub fn open_page(page: String) -> Result<(), String> {
    let url = url_of(&page).ok_or_else(|| format!("unknown page {page:?}"))?;
    open(url).map_err(|e| format!("could not start the browser: {e}"))
}

#[cfg(test)]
mod tests {
    use super::url_of;

    #[test]
    fn only_known_pages_open() {
        assert_eq!(url_of("new-issue"), Some("https://github.com/BenchMesh/benchmesh/issues/new"));
        assert!(url_of("issues").unwrap().ends_with("/issues"));
        assert_eq!(url_of("docs"), Some("https://benchmesh.online/docs"));
        assert_eq!(url_of("docs-scpi"), Some("https://benchmesh.online/docs/scpi"));
        assert_eq!(url_of("docs-python"), Some("https://benchmesh.online/docs/python"));
        assert_eq!(url_of("headless"), Some("https://benchmesh.online/docs/headless"));
        assert_eq!(url_of("linux-usb"), Some("https://benchmesh.online/docs/linux#usb-permissions"));
        assert_eq!(url_of("linux-serial"), Some("https://benchmesh.online/docs/linux#serial-permissions"));
        assert_eq!(url_of("macos-local-network"), Some("https://benchmesh.online/docs/macos#local-network"));
        assert!(url_of("macos-network-settings").unwrap().starts_with("x-apple.systempreferences:"));
        assert_eq!(url_of("email"), Some("mailto:support@benchmesh.online"));
        assert_eq!(url_of("https://example.com"), None);
        assert_eq!(url_of(""), None);
    }
}
