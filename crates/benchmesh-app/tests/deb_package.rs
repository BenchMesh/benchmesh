//! The `.deb` package (`tauri.conf.json` `bundle.linux.deb`): the udev rules that give USBTMC
//! instruments and USB serial ports to the signed-in user, and the maintainer scripts that apply
//! them. Checked here because a broken path or script only shows when the package is built.

use std::path::Path;

fn deb() -> serde_json::Value {
    let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    conf["bundle"]["linux"]["deb"].clone()
}

fn here(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

#[test]
fn the_package_installs_the_udev_rules() {
    let files = deb()["files"].as_object().unwrap().clone();
    let src = files["/usr/lib/udev/rules.d/70-benchmesh.rules"].as_str().unwrap();
    let rules = std::fs::read_to_string(here(src)).unwrap();
    let active: Vec<&str> = rules.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).collect();
    assert_eq!(
        active,
        [
            r#"SUBSYSTEM=="usbmisc", KERNEL=="usbtmc[0-9]*", MODE="0660", TAG+="uaccess""#,
            r#"SUBSYSTEM=="tty", KERNEL=="ttyUSB[0-9]*|ttyACM[0-9]*", TAG+="uaccess""#,
        ]
    );
    for (_, src) in &files {
        assert!(here(src.as_str().unwrap()).is_file(), "{src} is missing");
    }
}

#[test]
fn the_maintainer_scripts_exist_and_are_executable() {
    for key in ["postInstallScript", "postRemoveScript"] {
        let path = here(deb()[key].as_str().unwrap());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("#!/bin/sh\n"), "{key}");
        assert!(text.contains("udevadm control --reload-rules || true"), "{key}: a failed reload must not fail the install");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o111, 0, "{key} is not executable");
        }
    }
}
