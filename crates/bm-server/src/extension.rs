//! The extension point of a BenchMesh distribution (contract `ai_context/OPEN_CORE.md`).
//!
//! The open-source build has no extensions: `benchmesh-server` and the desktop app built from this
//! repository pass `Extensions::default()`. The official distribution adds its own (app updates,
//! online services, finding instruments, environmental sensors) at build time through the `benchmesh-extensions` crate,
//! which the desktop app asks for them. An extension adds routes to the same API: they are served behind the same
//! cross-site guard and remote gate, and their remote scope follows their path like every other
//! route (`remote::scope`; an unclassified path is local-only).

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use bm_devicedef::Link;
use bm_driver::actor::{Connector, Presence};
use bm_transport::Endpoint;
use serde_json::Value;

use crate::config::DeviceEntry;
use crate::environment::Environment;
use crate::S;

/// The distribution name of a build without extensions (`GET /version` `distribution`).
pub const COMMUNITY: &str = "community";

/// What an extension is told about the BenchMesh it runs in.
#[derive(Debug, Clone)]
pub struct Host {
    /// The user data directory (`None`: a simulation, everything in memory).
    pub data_dir: Option<PathBuf>,
    /// Running without an app window (`--headless`): the process may stop and start itself
    /// (`system::System::hand_over`).
    pub headless: bool,
}

/// One route of an extension as `/openapi.json` describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiRoute {
    pub method: &'static str,
    pub path: &'static str,
    /// One of the API's tags (`openapi::TAGS`).
    pub tag: &'static str,
    pub summary: &'static str,
    /// It takes a JSON body.
    pub body: bool,
    /// Why the API reference must not send it (`x-no-try`).
    pub no_try: Option<&'static str>,
}

pub trait Extension: Send + Sync {
    /// Short name for logs.
    fn name(&self) -> &'static str;
    /// Routes merged into the API.
    fn routes(&self) -> Router<S>;
    /// Every route of `routes()`, for `/openapi.json`.
    fn api(&self) -> Vec<ApiRoute>;
    /// Finds instruments by who they are; the open-source build has none.
    fn finder(&self) -> Option<Arc<dyn Finder>> {
        None
    }
    /// Reports the lab's temperature, humidity and pressure (`environment.rs`): every running
    /// recording records them while it does; the open-source build has none.
    fn environment(&self) -> Option<Arc<dyn Environment>> {
        None
    }
    /// The server is set up: background work that needs its state starts here (called once, on
    /// the server's runtime, so `tokio::spawn` works; return at once). A simulation is told by
    /// `Host::data_dir` being `None`.
    fn started(&self, _state: &S) {}
}

/// Finds a configured instrument by who it is (the entry's `match`) instead of the port it
/// names. Without one (the open-source build) an instrument is opened only on the `port` /
/// `device` its entry names, and `match` is not followed.
pub trait Finder: Send + Sync {
    /// Checks an entry's `match` (when it is added, changed or loaded).
    fn check(&self, m: &Value) -> Result<(), String>;
    /// The ports to try for an entry with `match`, asked again on every reconnect. `ep.path` is
    /// the port the entry names (a hint, may be empty).
    fn connector(&self, entry: &DeviceEntry, link: &Link, ep: &Endpoint) -> Connector;
    /// Where an entry with `match` is attached, without a byte to the instrument (parking).
    fn presence(&self, entry: &DeviceEntry, link: &Link, ep: &Endpoint) -> Presence;
}

impl fmt::Debug for dyn Extension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Extension({})", self.name())
    }
}

/// The extensions of this build and the distribution they make.
#[derive(Debug, Clone)]
pub struct Extensions {
    /// `community` for a build of the open-source repository; the official build names itself.
    pub distribution: &'static str,
    pub list: Vec<Arc<dyn Extension>>,
}

impl Default for Extensions {
    fn default() -> Self {
        Extensions { distribution: COMMUNITY, list: Vec::new() }
    }
}

impl Extensions {
    /// The documented routes of every extension.
    pub fn api(&self) -> Vec<ApiRoute> {
        self.list.iter().flat_map(|e| e.api()).collect()
    }

    /// The finder of the first extension that has one.
    pub fn finder(&self) -> Option<Arc<dyn Finder>> {
        self.list.iter().find_map(|e| e.finder())
    }

    /// Tells every extension the server is set up (`Extension::started`).
    pub fn started(&self, state: &S) {
        for e in &self.list {
            e.started(state);
        }
    }

    /// The environmental sensor of the first extension that has one.
    pub fn environment(&self) -> Option<Arc<dyn Environment>> {
        self.list.iter().find_map(|e| e.environment())
    }
}
