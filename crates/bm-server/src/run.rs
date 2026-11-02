//! Start/stop the server as a library (used by `benchmesh-server` and the desktop app).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bm_driver::actor::ActorConfig;

use crate::config::{validate_entry, ConfigFile};
use crate::devices::{hardware_connectors, hardware_presence, simulated_connectors, Devices};
use crate::{network_router, router, AppState};

#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// config.yaml (created on first save if missing).
    pub config: PathBuf,
    /// Device definitions directory (`devices/`).
    pub devices_dir: PathBuf,
    /// Built UI (`index.html` + assets) served under /ui, if any.
    pub ui_dir: Option<PathBuf>,
    /// Listen addresses tried in order (e.g. `127.0.0.1:57666`, then `127.0.0.1:0`).
    pub listen: Vec<String>,
    /// Recordings database (`None` = in memory, nothing persisted).
    pub recordings_db: Option<PathBuf>,
    /// Replay this capture session instead of opening real ports (no hardware needed).
    pub simulate: Option<PathBuf>,
    pub ws_interval: Duration,
    /// Flows and automation settings (`flows/`, `automation.json`); `None` = in memory.
    pub data_dir: Option<PathBuf>,
    /// Running without an app window: the UI may restart / shut down this process and make it
    /// start with the computer (`system/`). `None` in the desktop app.
    pub headless: Option<crate::system::Headless>,
    /// What the distribution adds (`extension.rs`); none in the open-source build.
    pub extensions: crate::extension::Extensions,
}

pub struct RunningServer {
    pub addr: SocketAddr,
    devices: Arc<Devices>,
    recordings: Arc<crate::recordings::Recordings>,
    remote: Arc<crate::remote::Remote>,
    scpi: Arc<crate::scpi::Scpi>,
    system: Arc<crate::system::System>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Problems with individual config entries (devices skipped at start).
    pub warnings: Vec<String>,
}

impl RunningServer {
    /// Base URL for local clients.
    pub fn url(&self) -> String {
        let host = if self.addr.ip().is_unspecified() { "127.0.0.1".to_string() } else { self.addr.ip().to_string() };
        format!("http://{host}:{}", self.addr.port())
    }

    /// Completes with what the UI asked a headless server to do (restart / shut down).
    pub async fn requested(&self) -> crate::system::Action {
        self.system.requested().await
    }

    /// Remote access: set its password and switch it on (headless start with a password).
    pub fn set_remote_password(&self, password: &str) -> Result<(), String> {
        if let Some(why) = crate::remote::password::refusal(password) {
            return Err(why);
        }
        if self.remote.is_on_with(password) {
            return Ok(());
        }
        self.remote.set_password(crate::remote::password::hash(password), true)
    }

    /// Remote access settings and state (addresses, certificate fingerprint).
    pub fn remote_status(&self) -> serde_json::Value {
        self.remote.status()
    }

    /// Stop serving, then stop every device (runs the definitions' on_disconnect steps).
    pub fn stop(mut self) {
        self.shutdown_now();
    }

    fn shutdown_now(&mut self) {
        self.remote.stop();
        self.scpi.stop();
        self.recordings.shutdown();
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.devices.stop_all();
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.shutdown_now();
    }
}

/// Load definitions + config, start one device thread per configured device and serve the
/// API/UI on a dedicated runtime thread. Returns once the listener is bound.
pub fn start(opts: ServerOptions) -> Result<RunningServer, String> {
    let catalog = bm_devicedef::load_catalog(&opts.devices_dir)
        .map_err(|e| format!("device definitions in {}:\n{}", opts.devices_dir.display(), e.join("\n")))?;
    let cfg = ConfigFile { path: opts.config.clone() };
    let entries = cfg.load()?;
    let connectors = match &opts.simulate {
        Some(session) => simulated_connectors(session.clone()),
        None => hardware_connectors(opts.extensions.finder()),
    };
    let mut devices = Devices::new(catalog, connectors, ActorConfig::default());
    devices.hardware = opts.simulate.is_none();
    if devices.hardware {
        devices.presence = hardware_presence(opts.extensions.finder());
    }
    let devices = Arc::new(devices);
    let mut warnings = Vec::new();
    let finder = opts.extensions.finder();
    for e in entries {
        let known = devices.driver_ids();
        match validate_entry(&e, &known, finder.as_deref()).and_then(|_| devices.start(e.clone())) {
            Ok(()) => {}
            Err(err) => warnings.push(format!("device {:?} not started: {err}", e.get("id"))),
        }
    }
    let store = match &opts.recordings_db {
        Some(p) => bm_store::Store::open(p),
        None => bm_store::Store::in_memory(),
    }
    .map_err(|e| format!("recordings database: {e}"))?;
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("bm-http").build().map_err(|e| e.to_string())?;
    let recordings = {
        let _guard = rt.enter(); // recording tasks are spawned on this runtime
        crate::recordings::Recordings::new(Arc::new(store), devices.clone(), opts.extensions.environment())
    };
    let state = Arc::new(AppState {
        devices: devices.clone(),
        recordings: recordings.clone(),
        config: cfg,
        ui_dir: opts.ui_dir.clone().filter(|p| p.join("index.html").exists()),
        ws_interval: opts.ws_interval,
        metrics_interval: Duration::from_secs(30),
        config_lock: Default::default(),
        community: Default::default(),
        automation: crate::automation::Automation::new(devices.clone(), recordings.clone(), opts.data_dir.clone()),
        remote: Arc::new(crate::remote::Remote::new(opts.data_dir.clone())),
        scpi: Arc::new(crate::scpi::Scpi::new(opts.data_dir.clone())),
        system: Arc::new(crate::system::System::new(opts.data_dir.clone(), opts.headless.clone())),
        extensions: opts.extensions.clone(),
    });
    for e in &state.extensions.list {
        eprintln!("extension: {}", e.name());
    }
    // community status of the models (website, read-only); a simulation stays offline
    if opts.simulate.is_none() {
        let c = state.community.clone();
        rt.spawn(async move { c.refresh().await });
    }
    {
        let _guard = rt.enter();
        state.extensions.started(&state);
    }

    let listener = rt.block_on(async {
        let mut last = String::from("no listen address given");
        for a in &opts.listen {
            match tokio::net::TcpListener::bind(a).await {
                Ok(l) => return Ok(l),
                Err(e) => last = format!("{a}: {e}"),
            }
        }
        Err(last)
    });
    let listener = match listener {
        Ok(l) => l,
        Err(e) => {
            state.remote.stop();
            devices.stop_all();
            return Err(format!("cannot listen: {e}"));
        }
    };
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let remote = state.remote.clone();
    let scpi = state.scpi.clone();
    let system = state.system.clone();
    // virtual SCPI instruments: their ports open now if they are switched on
    scpi.attach(Arc::downgrade(&state), rt.handle().clone());
    // remote access: the network listener opens now if it is switched on, or later from the UI
    remote.attach(network_router(state.clone()), rt.handle().clone());
    let app = router(state);
    let thread = std::thread::Builder::new()
        .name("bm-server".into())
        .spawn(move || {
            rt.block_on(async move {
                let _ = axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = rx.await;
                    })
                    .await;
            });
        })
        .map_err(|e| e.to_string())?;
    Ok(RunningServer { addr, devices, recordings, remote, scpi, system, shutdown: Some(tx), thread: Some(thread), warnings })
}
