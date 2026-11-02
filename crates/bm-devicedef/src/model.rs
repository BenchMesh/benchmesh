//! Data model of the BenchMesh device-definition DSL (`benchmesh.device/v1`) and the
//! instrument class schema (`benchmesh.class/v1`).
//!
//! These types are the single source of truth: the JSON Schema is generated from them
//! (`bm-devicedef schema`), and the interpreter consumes them. Unknown keys are rejected
//! everywhere so that typos in hand-written or LLM-generated definitions fail loudly.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const DEVICE_SCHEMA_ID: &str = "benchmesh.device/v1";
pub const CLASS_SCHEMA_ID: &str = "benchmesh.class/v1";

// ---------------------------------------------------------------------------------------
// Device definition
// ---------------------------------------------------------------------------------------

/// One device family (e.g. all TENMA 72-series PSUs sharing a protocol).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceDef {
    /// Must be `benchmesh.device/v1`.
    pub schema: String,
    /// Unique definition id, also the config `driver` key (e.g. `tenma_72`).
    pub id: String,
    /// Base definition (`devices/bases/<id>.yaml`) this one was merged over (see `DEVICE_DSL.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    pub vendor: String,
    pub family: String,
    /// Definition version (semver).
    pub version: String,
    /// Hide work-in-progress definitions from the configuration UI.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Provenance: manuals (with pages) and captures this definition was derived from.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// Supported models, matched against the identify reply.
    pub models: Vec<Model>,
    /// Command used to identify the device on connect (its reply is matched against `models`).
    pub identify: String,
    pub link: Links,
    pub framing: Framing,
    #[serde(default)]
    pub session: Session,
    /// Client-side state for values the device cannot report (shadow state).
    #[serde(default)]
    pub state: BTreeMap<String, StateVar>,
    pub commands: BTreeMap<String, Command>,
    /// Mapping of this device onto instrument classes (PSU, DMM, ...).
    pub classes: BTreeMap<String, ClassBinding>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Document title or file name.
    #[serde(default)]
    pub doc: Option<String>,
    /// Page reference, e.g. `12-14`.
    #[serde(default)]
    pub pages: Option<String>,
    /// Capture session directory used as ground truth.
    #[serde(default)]
    pub capture: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub name: String,
    /// Regular expression matched against the identify reply.
    #[serde(rename = "match")]
    pub pattern: String,
    /// Per-class feature overrides merged over the class binding `features`.
    #[serde(default)]
    pub features: BTreeMap<String, serde_json::Value>,
    /// Per-class channel count overrides.
    #[serde(default)]
    pub channels: BTreeMap<String, u8>,
    /// How far this model is known to work (`ai_context/migration/coverage/README.md`).
    #[serde(default)]
    pub support: Support,
    #[serde(default)]
    pub note: Option<String>,
}

/// Verification status of a model. The community status will come from the online service;
/// a definition carries the status known when it was written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    /// Verified live on the maintainers' bench (`bm-probe verify` + setter checks).
    Tested,
    /// A user ran the first-contact check on the real instrument and it passed.
    Confirmed,
    /// Written from the manual (or shared with a tested family); not yet seen live.
    #[default]
    Awaiting,
}

impl Support {
    pub fn as_str(self) -> &'static str {
        match self {
            Support::Tested => "tested",
            Support::Confirmed => "confirmed",
            Support::Awaiting => "awaiting",
        }
    }
}

// --- link ------------------------------------------------------------------------------

/// The physical links a family offers (at least one), e.g.
/// `link: {usbtmc: {vid: "1ab1"}, tcp: {port: 5555}}`. A configured instrument uses one of them:
/// its config `transport`, or the only one. Each may replace the top-level `framing`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Links {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<SerialLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usbtmc: Option<UsbtmcLink>,
    /// Raw socket (SCPI over LAN, usually port 5025 or 5555).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp: Option<TcpLink>,
    /// VXI-11 (ONC RPC over LAN; LXI instruments without a raw socket). Message-based like USBTMC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vxi11: Option<Vxi11Link>,
}

/// One link of a definition, the one an instrument is reached through.
#[derive(Debug, Clone)]
pub enum Link {
    Serial(SerialLink),
    Usbtmc(UsbtmcLink),
    Tcp(TcpLink),
    Vxi11(Vxi11Link),
}

impl Link {
    /// Config `transport` name: `serial`, `usbtmc`, `tcpip`.
    pub fn kind(&self) -> &'static str {
        match self {
            Link::Serial(_) => "serial",
            Link::Usbtmc(_) => "usbtmc",
            Link::Tcp(_) => "tcpip",
            Link::Vxi11(_) => "vxi11",
        }
    }

    /// The link's own framing (replacing the top-level one), if any.
    pub fn own_framing(&self) -> Option<&Framing> {
        match self {
            Link::Serial(l) => l.framing.as_ref(),
            Link::Usbtmc(l) => l.framing.as_ref(),
            Link::Tcp(l) => l.framing.as_ref(),
            Link::Vxi11(l) => l.framing.as_ref(),
        }
    }
}

impl Links {
    pub fn all(&self) -> Vec<Link> {
        let mut out = Vec::new();
        out.extend(self.serial.clone().map(Link::Serial));
        out.extend(self.usbtmc.clone().map(Link::Usbtmc));
        out.extend(self.tcp.clone().map(Link::Tcp));
        out.extend(self.vxi11.clone().map(Link::Vxi11));
        out
    }

    /// Config `transport` names of the links offered.
    pub fn kinds(&self) -> Vec<&'static str> {
        self.all().iter().map(Link::kind).collect()
    }

    /// The link for `transport` (config name), or without one the only link - with several,
    /// serial, then USB-TMC (configs without a transport have always meant the local link;
    /// a LAN link needs a host and is only used when asked for).
    pub fn choose(&self, transport: Option<&str>) -> Result<Link, String> {
        let all = self.all();
        let found = match transport {
            Some(t) => all.into_iter().find(|l| l.kind() == t),
            None if all.len() == 1 => all.into_iter().next(),
            None => {
                let local = |want: &str| all.iter().find(|l| l.kind() == want).cloned();
                local("serial").or_else(|| local("usbtmc"))
            }
        };
        found.ok_or_else(|| match transport {
            Some(t) => format!("transport {t:?} is not offered (offered: {})", self.kinds().join(", ")),
            None => format!("choose a transport ({})", self.kinds().join(", ")),
        })
    }

    /// The only link; `Links` of a definition resolved with [`DeviceDef::for_transport`] have one.
    pub fn single(&self) -> Option<Link> {
        let all = self.all();
        (all.len() == 1).then(|| all.into_iter().next()).flatten()
    }
}

impl DeviceDef {
    /// This definition as one configured instrument uses it: only the chosen link, and that
    /// link's framing when it has its own.
    pub fn for_transport(&self, transport: Option<&str>) -> Result<DeviceDef, String> {
        let link = self.link.choose(transport)?;
        let mut d = self.clone();
        if let Some(f) = link.own_framing() {
            d.framing = f.clone();
        }
        d.link = match link {
            Link::Serial(l) => Links { serial: Some(l), ..Links::default() },
            Link::Usbtmc(l) => Links { usbtmc: Some(l), ..Links::default() },
            Link::Tcp(l) => Links { tcp: Some(l), ..Links::default() },
            Link::Vxi11(l) => Links { vxi11: Some(l), ..Links::default() },
        };
        Ok(d)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SerialLink {
    pub baud: u32,
    /// Data bits, parity, stop bits, e.g. `8N1`.
    #[serde(default = "default_serial_format")]
    pub format: String,
    #[serde(default)]
    pub flow: Flow,
    #[serde(default)]
    pub dtr: LineState,
    #[serde(default)]
    pub rts: LineState,
    /// Wait after opening the port before the first command.
    #[serde(default)]
    pub settle_ms: u32,
    /// Whether failing to set DTR/RTS aborts the connection.
    #[serde(default)]
    pub modem_control: ModemControl,
    /// Replaces the top-level `framing` on this link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framing: Option<Framing>,
}

fn default_serial_format() -> String {
    "8N1".into()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    #[default]
    None,
    RtsCts,
    XonXoff,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LineState {
    #[default]
    Untouched,
    Low,
    High,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModemControl {
    #[default]
    BestEffort,
    Required,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UsbtmcLink {
    /// USB vendor id (hex), used for discovery.
    #[serde(default)]
    pub vid: Option<String>,
    /// USB product id (hex), used for discovery.
    #[serde(default)]
    pub pid: Option<String>,
    /// Replaces the top-level `framing` on this link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framing: Option<Framing>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TcpLink {
    /// Default port (a config entry's `host` may carry another: `host:port`).
    pub port: u16,
    /// Replaces the top-level `framing` on this link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framing: Option<Framing>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Vxi11Link {
    /// Logical device name on the instrument (`inst0`; GPIB gateways: `gpib0,5`).
    #[serde(default = "default_vxi11_device")]
    pub device: String,
    /// Replaces the top-level `framing` on this link (usually `rx.mode: message`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framing: Option<Framing>,
}

fn default_vxi11_device() -> String {
    "inst0".into()
}

// --- framing ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Framing {
    pub tx: TxFraming,
    pub rx: RxFraming,
    #[serde(default)]
    pub timing: Timing,
    /// Modbus RTU: commands with `modbus` are binary request frames with CRC16 to this slave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modbus: Option<ModbusFraming>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModbusFraming {
    /// Slave address (1..247).
    #[serde(default = "default_modbus_address")]
    pub address: u8,
}

fn default_modbus_address() -> u8 {
    1
}

/// One Modbus RTU request. Reads return the data bytes (decoded with `word` / `registers` /
/// `bits`); writes return nothing (the echo is checked).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModbusOp {
    #[serde(rename = "fn")]
    pub function: ModbusFn,
    /// First register / coil address (0-based, as on the wire).
    pub register: u16,
    /// Reads: number of registers / coils (default 1, or 2 for 32-bit `encoding`).
    #[serde(default)]
    pub count: Option<u16>,
    /// Writes: the param holding the value (after its `scale`); coils take ON/OFF/1/0.
    #[serde(default)]
    pub value: Option<String>,
    /// Writes: how the value is encoded (`write_single` takes 16-bit types only).
    #[serde(default)]
    pub encoding: WordType,
    /// 32-bit values with the low word first.
    #[serde(default)]
    pub swap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModbusFn {
    /// fc 01
    ReadCoils,
    /// fc 02
    ReadDiscrete,
    /// fc 03
    ReadHolding,
    /// fc 04
    ReadInput,
    /// fc 05
    WriteCoil,
    /// fc 06
    WriteSingle,
    /// fc 16
    WriteMultiple,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WordType {
    #[default]
    U16,
    I16,
    U32,
    I32,
    F32,
}

impl WordType {
    pub fn words(self) -> u16 {
        match self {
            WordType::U16 | WordType::I16 => 1,
            _ => 2,
        }
    }
}

/// A value at a register offset of a Modbus read (big-endian words).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterField {
    /// Word index in the reply (0 = the first register read).
    pub at: usize,
    #[serde(rename = "type", default)]
    pub kind: WordType,
    /// Multiply the raw integer (e.g. 0.01 for centivolts).
    #[serde(default)]
    pub scale: Option<f64>,
    /// 32-bit: low word first.
    #[serde(default)]
    pub swap: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TxFraming {
    /// Appended to every command (may be empty). Escapes: `\r`, `\n`.
    pub terminator: String,
}

/// How a reply is delimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RxMode {
    /// Reply ends with `terminator`.
    Terminator,
    /// Reply length is fixed per command (`reply: {bytes: N}`); no terminator.
    Fixed,
    /// Reply ends after `idle_ms` of silence.
    Idle,
    /// Transport delivers whole messages (USBTMC).
    Message,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RxFraming {
    pub mode: RxMode,
    /// Required for `mode: terminator`.
    #[serde(default)]
    pub terminator: Option<String>,
    /// Characters discarded before a reply starts (e.g. a stray `\n` of a `\r\n` reply).
    #[serde(default)]
    pub skip_leading: String,
    /// Characters stripped from the end of every reply.
    #[serde(default)]
    pub strip_trailing: String,
    /// Maximum time to wait for a complete reply.
    pub timeout_ms: u32,
    /// Required for `mode: idle`.
    #[serde(default)]
    pub idle_ms: Option<u32>,
    /// After a timeout or malformed reply, discard input until the line is idle this long
    /// (prevents replies from shifting onto the next command).
    #[serde(default)]
    pub drain_idle_ms: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    /// Minimum gap between the end of one exchange and the next command.
    #[serde(default)]
    pub min_gap_ms: u32,
    /// Gap after a command that produces no reply (setters), before the next command.
    #[serde(default)]
    pub gap_after_write_ms: u32,
}

// --- session ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Session {
    /// Steps run after identify succeeds (e.g. enter remote mode).
    #[serde(default)]
    pub on_connect: Vec<Step>,
    /// Steps run before closing the link (e.g. return to local mode).
    #[serde(default)]
    pub on_disconnect: Vec<Step>,
}

// --- state -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateVar {
    #[serde(rename = "type")]
    pub kind: ValueKind,
    #[serde(default)]
    pub initial: Option<serde_json::Value>,
    #[serde(default)]
    pub description: Option<String>,
}

// --- commands --------------------------------------------------------------------------

/// One wire command. `send` is a template: `{ch}` channel, `{<param>}` a parameter,
/// `{state.<var>}` a state variable.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Command {
    /// Wire text template (empty for a `modbus` command).
    #[serde(default)]
    pub send: String,
    /// Modbus RTU request instead of `send` (framing `modbus` required).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modbus: Option<ModbusOp>,
    #[serde(default)]
    pub params: BTreeMap<String, Param>,
    #[serde(default)]
    pub reply: Reply,
    /// Reply parse pipeline, applied in order.
    #[serde(default)]
    pub parse: Vec<ParseStep>,
    /// Result cache lifetime.
    #[serde(default)]
    pub cache_ms: Option<u32>,
    /// Cached commands invalidated when this command runs.
    #[serde(default)]
    pub invalidates: Vec<String>,
    /// State updates after success: state var -> template; placeholders receive the
    /// caller's (API) values, not the mapped wire text.
    #[serde(default)]
    pub effects: BTreeMap<String, String>,
    /// Steps that must run first (e.g. switch mode before a mode-gated setpoint).
    #[serde(default)]
    pub before: Vec<Step>,
    /// Refuse to run unless this CEL condition holds (evaluated over cached query results
    /// and state). Used instead of implicit side effects, e.g. mode-gated setpoints.
    #[serde(default)]
    pub requires: Option<Requirement>,
    #[serde(default)]
    pub safety: Safety,
    /// What an empty reply means: an error (default) or "no value" (e.g. local mode).
    #[serde(default)]
    pub on_empty: OnEmpty,
    /// Reply timeout override.
    #[serde(default)]
    pub timeout_ms: Option<u32>,
    /// Maximum reply size (block replies).
    #[serde(default)]
    pub max_bytes: Option<u32>,
    /// Gap after this command before the next one (overrides framing timing).
    #[serde(default)]
    pub gap_after_ms: Option<u32>,
    /// After this command, polled readings are reported as null for this long
    /// (instrument needs time to produce valid readings, e.g. after a mode change).
    #[serde(default)]
    pub settle_ms: Option<u32>,
    #[serde(default)]
    pub description: Option<String>,
    /// Manual reference, e.g. `programming manual p.12`.
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    /// CEL condition over cached query results (by command name), `state` and `ch`.
    pub expr: String,
    /// Error returned to the caller when the condition is false.
    pub message: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnEmpty {
    #[default]
    Error,
    Null,
}

/// Reply framing for one command. Unit variants are plain strings, e.g. `reply: none`;
/// fixed length is `reply: {bytes: 5}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    /// One reply framed by the definition's rx framing.
    #[default]
    Line,
    /// No reply: never read after this command.
    None,
    /// Exactly N bytes; completes as soon as N bytes arrived (no terminator wait).
    Bytes(u32),
    /// IEEE 488.2 definite-length binary block (`#<n><len><data>`).
    Block,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Safety {
    /// Reads only.
    #[default]
    Query,
    /// Changes configuration but cannot energise anything.
    Configure,
    /// Can energise an output/input (PSU output on, load input on, AWG output on).
    Energize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Param {
    #[serde(rename = "type")]
    pub kind: ValueKind,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    /// printf-style wire format for numbers: `%.2f`, `%.3f`, `%d`, `%03d` (zero-padded), `%g`.
    #[serde(default)]
    pub format: Option<String>,
    /// Wire value = API value × scale (after the min/max check), for devices that take scaled
    /// integers: Manson `VOLT127` is 12.7 V with `scale: 10, format: "%03d"`.
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
    /// For `type: enum`: API value -> wire text.
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    /// For `type: string`: regex the value must match.
    #[serde(default)]
    pub pattern: Option<String>,
    /// Computed param: CEL expression evaluated at send time over cached query results,
    /// `state` and `ch` (callers never supply it). With `type: enum`, the result is
    /// mapped through `values`.
    #[serde(default)]
    pub from: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    Number,
    Integer,
    Bool,
    String,
    Enum,
    /// BenchMesh SI value dict `{si:{number,symbol?,prefix?}, sci, val}` (classes only).
    Si,
    /// Binary payload (e.g. a screenshot), served as a file (classes only).
    Bytes,
    /// Array of numbers, e.g. waveform samples (classes only).
    List,
}

impl ValueKind {
    /// Whether `v` is a value of this kind (`values`: what an enum allows). A class contract's
    /// check of a status value or an operation's result.
    pub fn accepts(self, values: &[String], v: &serde_json::Value) -> bool {
        match self {
            ValueKind::Si => v.get("si").is_some() || v.get("val").is_some(),
            ValueKind::Number => v.is_number(),
            ValueKind::Integer => v.as_i64().is_some() || v.as_f64().is_some_and(|f| f.fract() == 0.0),
            ValueKind::Bool => v.is_boolean(),
            ValueKind::Enum => v.as_str().is_some_and(|s| values.iter().any(|x| x == s)),
            ValueKind::String => v.is_string(),
            ValueKind::List => v.is_array(),
            ValueKind::Bytes => true,
        }
    }
}

/// One reply transformation. Unit steps are plain strings (`- trim`); others are maps
/// (`- {split: {sep: ",", index: 2}}`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ParseStep {
    /// Strip ASCII whitespace at both ends.
    Trim,
    /// Remove one pair of surrounding double quotes.
    Unquote,
    Lower,
    Upper,
    /// Parse as a floating point number.
    Number,
    /// A number as its shortest decimal text (`1.000000e+01` -> `"10"`, `0.1` -> `"0.1"`): a
    /// value the class contract wants as text, written like the setter's values.
    Text,
    /// Multiply a number (scaled-integer replies: `{scale: 0.01}` turns `1270` into 12.7).
    Scale(f64),
    /// ASCII list of numbers → list (`-80.1,-79.9,…`): separated by commas, semicolons or
    /// whitespace/newlines; empty fields (trailing comma) are skipped; an IEEE 488.2 header
    /// (`#9000001234`) before the list is dropped. Spectrum analyzer / VNA traces, DAQ scans.
    Numbers,
    /// Take the first byte as an unsigned integer (binary replies).
    Byte,
    /// Split and keep one field.
    Split { sep: String, index: usize },
    /// Split into named fields (result becomes a map).
    Fields { sep: String, names: Vec<String> },
    /// Regex capture group (1-based; 0 = whole match).
    Regex { pattern: String, group: usize },
    Replace { from: String, to: String },
    /// Exact-match mapping; unmapped values are an error unless `_` is present. `{raw}` in a
    /// value stands for the input text (`_: "{raw}"` passes unlisted values through).
    Map(BTreeMap<String, String>),
    /// Bit fields from an integer: name -> `N` (one bit, bool) or `A..B` (inclusive, integer).
    Bits(BTreeMap<String, String>),
    /// Raw replies that mean "no value" (result becomes null).
    NullIf(Vec<String>),
    /// Binary sample data (e.g. an oscilloscope's block payload) → list of integers.
    Samples(SampleFormat),
    /// One value from register data (Modbus read): `{word: {at: 0, type: u16, scale: 0.01}}`.
    Word(RegisterField),
    /// Named values from register data → map: `{registers: {vout: {at: 2, scale: 0.01}}}`.
    Registers(BTreeMap<String, RegisterField>),
}

/// Encoding of one sample in a binary `samples` reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SampleFormat {
    U8,
    I8,
    U16le,
    I16le,
    U16be,
    I16be,
}

/// A command invocation: `cmd_name` or `{cmd: name, args: {param: value}, only: [values]}`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Step {
    Name(String),
    Call {
        cmd: String,
        #[serde(default)]
        args: BTreeMap<String, String>,
        /// Run this step of a setter only for these caller values (others skip it), e.g. a
        /// series/parallel command that exists only for some LCR parameter pairs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        only: Option<Vec<String>>,
    },
}

impl Step {
    pub fn command(&self) -> &str {
        match self {
            Step::Name(n) => n,
            Step::Call { cmd, .. } => cmd,
        }
    }
}

// --- class binding ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassBinding {
    #[serde(default = "default_channels")]
    pub channels: u8,
    pub ui_component: String,
    /// UI feature dictionary served by `GET /instruments/{class}/{id}`.
    #[serde(default)]
    pub features: serde_json::Value,
    /// Poll groups (e.g. a fast and a slow tier). Results of all groups are CEL variables
    /// named after their commands; a slower group's values persist between its refreshes.
    pub poll: Vec<PollGroup>,
    /// Class status key -> value. Shorthand string = CEL expression with `as: raw`.
    pub status: BTreeMap<String, StatusField>,
    /// API operation (partial name) -> get/set actions.
    #[serde(default)]
    pub operations: BTreeMap<String, Operation>,
}

fn default_channels() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PollGroup {
    pub every_ms: u32,
    /// Query commands run each cycle of this group (per channel when `{ch}` is used).
    pub read: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum StatusField {
    Expr(String),
    Full {
        expr: String,
        #[serde(rename = "as", default)]
        repr: Repr,
    },
}

impl StatusField {
    pub fn expr(&self) -> &str {
        match self {
            StatusField::Expr(e) => e,
            StatusField::Full { expr, .. } => expr,
        }
    }
    pub fn repr(&self) -> Repr {
        match self {
            StatusField::Expr(_) => Repr::Raw,
            StatusField::Full { repr, .. } => *repr,
        }
    }
}

/// Wire representation of a value in API/registry payloads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Repr {
    /// Value as computed (string/number/bool).
    #[default]
    Raw,
    /// BenchMesh SI value dict.
    Si,
    /// Binary payload served as a file.
    Bytes,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    /// Command (or steps) answering `GET .../{op}`.
    #[serde(default)]
    pub get: Option<Action>,
    /// Command (or steps) performing `POST .../{op}/{value}`; the value binds to the
    /// command's single parameter, or to `{value}` in step args.
    #[serde(default)]
    pub set: Option<Action>,
    /// Representation of the GET result.
    #[serde(rename = "as", default)]
    pub repr: Repr,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Action {
    Command(String),
    Steps(Vec<Step>),
}

impl Action {
    pub fn steps(&self) -> Vec<Step> {
        match self {
            Action::Command(c) => vec![Step::Name(c.clone())],
            Action::Steps(s) => s.clone(),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Class schema
// ---------------------------------------------------------------------------------------

/// Contract of an instrument class (what the UI/API rely on).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassDef {
    /// Must be `benchmesh.class/v1`.
    pub schema: String,
    /// 3-letter class code.
    pub class: String,
    pub description: String,
    pub status: BTreeMap<String, StatusSpec>,
    #[serde(default)]
    pub operations: BTreeMap<String, OpSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusSpec {
    #[serde(rename = "type")]
    pub kind: ValueKind,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpSpec {
    #[serde(default)]
    pub get: bool,
    #[serde(default)]
    pub set: bool,
    /// Value type of the POST parameter / GET result.
    pub value: ValueKind,
    #[serde(default)]
    pub values: Vec<String>,
    /// Directions every device binding must provide, e.g. `[get, set]` or `[set]`.
    #[serde(default)]
    pub required: Vec<Direction>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Get,
    Set,
}

#[cfg(test)]
mod tests {
    use super::ValueKind;
    use serde_json::json;

    #[test]
    fn a_kind_accepts_its_values_only() {
        let on_off = ["ON".to_string(), "OFF".to_string()];
        assert!(ValueKind::Enum.accepts(&on_off, &json!("ON")));
        assert!(!ValueKind::Enum.accepts(&on_off, &json!("on")));
        assert!(ValueKind::Si.accepts(&[], &json!({"si": {"number": "5.00", "symbol": "V"}, "val": 5.0})));
        assert!(!ValueKind::Si.accepts(&[], &json!(5.0)));
        assert!(ValueKind::Number.accepts(&[], &json!(1.5)) && !ValueKind::Number.accepts(&[], &json!("1.5")));
        assert!(ValueKind::Integer.accepts(&[], &json!(3.0)) && !ValueKind::Integer.accepts(&[], &json!(3.5)));
        assert!(ValueKind::Bool.accepts(&[], &json!(true)) && !ValueKind::Bool.accepts(&[], &json!("ON")));
        assert!(ValueKind::List.accepts(&[], &json!([1, 2])) && ValueKind::String.accepts(&[], &json!("x")));
    }
}
