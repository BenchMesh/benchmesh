//! Per-device engine: executes a definition over a [`Transport`].
//!
//! Synchronous and single-owner by design: one thread owns one device (see
//! [`crate::actor`]), so there is no locking and replies can never interleave.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use bm_devicedef::template::{self, Piece};
use bm_devicedef::{Command, DeviceDef, Model, Reply, RxMode, Step};
use regex::Regex;
use serde_json::Value;

use crate::expr::{Expr, Vars};
use crate::framing::Framer;
use crate::parse::parse_reply;
use crate::status::{represent, CompiledStatus};
use crate::wire::render;
use crate::Error;

/// Byte transport to one instrument.
pub trait Transport: Send {
    fn write(&mut self, data: &[u8]) -> std::io::Result<()>;
    /// Bytes that arrived within `timeout`; empty = nothing arrived. Message-based
    /// transports (USBTMC) return exactly one device message per call.
    fn read(&mut self, timeout: Duration) -> std::io::Result<Vec<u8>>;
    /// What the transport did on its own since the last call (a USB recovery, say), for the
    /// instrument's wire log.
    fn take_notes(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// A read asks the instrument for its next message (USBTMC `REQUEST_DEV_DEP_MSG_IN`,
    /// VXI-11 `device_read`) instead of taking bytes off a line. Such a transport is read only
    /// right after a query: a read request with nothing to answer can hang the instrument until
    /// it is switched off and on (the Rigol DHO804, 2026-10-21 and the 0.1.19 diagnostic).
    fn reads_on_request(&self) -> bool {
        false
    }
}

/// Result of an operation.
#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Value(Value),
    Bytes(Vec<u8>),
}

impl Output {
    pub fn value(self) -> Value {
        match self {
            Output::Value(v) => v,
            Output::Bytes(b) => Value::String(format!("<{} bytes>", b.len())),
        }
    }
}

/// Status of one class channel after a poll.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusUpdate {
    pub class: String,
    pub ch: u8,
    pub status: BTreeMap<String, Value>,
}

fn ms(v: u32) -> Duration {
    Duration::from_millis(v as u64)
}

struct Scheduled {
    class: String,
    group: usize,
    next: Instant,
}

pub struct Engine {
    def: Arc<DeviceDef>,
    t: Box<dyn Transport>,
    pub idn: String,
    pub model: Model,
    state: BTreeMap<String, Value>,
    cache: HashMap<(String, u8), (Value, Instant)>,
    latest: BTreeMap<u8, BTreeMap<String, Value>>,
    compiled: BTreeMap<String, CompiledStatus>,
    schedule: Vec<Scheduled>,
    next_write: Instant,
    settle_until: Option<Instant>,
    /// Consecutive polls in which every command timed out.
    pub silent_polls: u32,
    /// Exchange counters since connect (for metrics).
    pub stats: ExchangeStats,
    /// An exchange lost the transport (an I/O error): the link has to be opened again.
    broken: bool,
}

/// What came back to a raw write ([`Engine::raw`]): every byte until the listen time ran out or
/// the line was quiet for the idle time after the first byte.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawReply {
    /// What was already waiting on the line before the write (a late reply to an earlier command).
    pub stale: Vec<u8>,
    pub bytes: Vec<u8>,
    /// From the write to the first and the last byte.
    pub first_ms: Option<u64>,
    pub last_ms: Option<u64>,
}

/// Wire-level counters (monotonic since connect).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ExchangeStats {
    pub exchanges: u64,
    pub failures: u64,
    /// Time the link was busy with exchanges (write → reply, incl. gaps).
    pub busy: Duration,
}

impl Engine {
    /// Identify the device, match the model, run `session.on_connect`.
    pub fn connect(def: Arc<DeviceDef>, transport: Box<dyn Transport>) -> Result<Engine, Error> {
        let mut e = Engine::open(def, transport)?;
        e.start_session()?;
        Ok(e)
    }

    /// Identify the device and match the model; nothing else is sent. Callers check
    /// [`Engine::idn`] (e.g. the instrument's serial number) before [`Engine::start_session`].
    pub fn open(def: Arc<DeviceDef>, transport: Box<dyn Transport>) -> Result<Engine, Error> {
        let compiled = def
            .classes
            .iter()
            .map(|(k, b)| Ok((k.clone(), CompiledStatus::new(b)?)))
            .collect::<Result<_, Error>>()?;
        let state = def.state.iter().map(|(k, v)| (k.clone(), v.initial.clone().unwrap_or(Value::Null))).collect();
        let now = Instant::now();
        let schedule = def
            .classes
            .iter()
            .flat_map(|(c, b)| (0..b.poll.len()).map(move |g| Scheduled { class: c.clone(), group: g, next: now }))
            .collect();
        let mut e = Engine {
            idn: String::new(),
            model: def.models[0].clone(),
            def: def.clone(),
            t: transport,
            state,
            cache: HashMap::new(),
            latest: BTreeMap::new(),
            compiled,
            schedule,
            next_write: now,
            settle_until: None,
            silent_polls: 0,
            stats: ExchangeStats::default(),
            broken: false,
        };
        // a connect banner (tinySA / NanoVNA shells) must not become the identify reply
        e.drain()?;
        let idn = e.run(&def.identify, 1, &BTreeMap::new())?.value();
        // a register (Modbus model id) identifies as its number's text
        e.idn = match idn {
            Value::String(s) => s,
            Value::Null => String::new(),
            v => v.to_string(),
        };
        e.model = def
            .models
            .iter()
            .find(|m| Regex::new(&m.pattern).map(|r| r.is_match(&e.idn)).unwrap_or(false))
            .cloned()
            .ok_or_else(|| Error::UnknownModel(e.idn.clone()))?;
        Ok(e)
    }

    /// Run `session.on_connect` (e.g. switch the instrument to remote mode).
    pub fn start_session(&mut self) -> Result<(), Error> {
        for s in self.def.session.on_connect.clone() {
            self.run_step(&s, 1, None)?;
        }
        Ok(())
    }

    /// Run `session.on_disconnect` (best effort).
    pub fn disconnect(mut self) {
        for s in self.def.session.on_disconnect.clone() {
            let _ = self.run_step(&s, 1, None);
        }
    }

    pub fn definition(&self) -> &DeviceDef {
        &self.def
    }

    pub fn channels(&self, class: &str) -> Option<u8> {
        let b = self.def.classes.get(class)?;
        Some(*self.model.channels.get(class).unwrap_or(&b.channels))
    }

    /// When the next poll group is due.
    pub fn next_due(&self) -> Option<Instant> {
        self.schedule.iter().map(|s| s.next).min()
    }

    // ---- operations ------------------------------------------------------------------

    pub fn get(&mut self, class: &str, op: &str, ch: u8) -> Result<Output, Error> {
        self.check_channel(class, ch)?;
        let oper = self.def.classes[class].operations.get(op).cloned();
        let (action, repr) = match oper.and_then(|o| o.get.map(|g| (g, o.repr))) {
            Some(x) => x,
            None => return Err(Error::NotFound(format!("{class} has no readable operation `{op}`"))),
        };
        let steps = action.steps();
        let mut out = Output::Value(Value::Null);
        for s in &steps {
            out = self.run_step(s, ch, None)?;
        }
        Ok(match out {
            Output::Value(v) => Output::Value(represent(v, repr)),
            b => b,
        })
    }

    pub fn set(&mut self, class: &str, op: &str, ch: u8, value: &str) -> Result<(), Error> {
        self.check_channel(class, ch)?;
        let action = match self.def.classes[class].operations.get(op).and_then(|o| o.set.clone()) {
            Some(a) => a,
            None => return Err(Error::NotFound(format!("{class} has no settable operation `{op}`"))),
        };
        for s in action.steps() {
            self.run_step(&s, ch, Some(value))?;
        }
        // Show the effect on the next poll instead of after the slowest group's interval.
        let now = Instant::now();
        for slot in self.schedule.iter_mut().filter(|slot| slot.class == class) {
            slot.next = now;
        }
        Ok(())
    }

    fn check_channel(&self, class: &str, ch: u8) -> Result<(), Error> {
        match self.channels(class) {
            None => Err(Error::NotFound(format!("device has no class {class}"))),
            Some(n) if ch == 0 || ch > n => Err(Error::BadArgument(format!("channel {ch} out of range 1..={n}"))),
            _ => Ok(()),
        }
    }

    // ---- polling ---------------------------------------------------------------------

    /// Run every poll group that is due; returns the refreshed class statuses.
    /// Timeouts become null readings; transport (I/O) errors are returned.
    pub fn poll_due(&mut self) -> Result<Vec<StatusUpdate>, Error> {
        let now = Instant::now();
        let due: Vec<usize> = (0..self.schedule.len()).filter(|&i| self.schedule[i].next <= now).collect();
        let mut updates = Vec::new();
        let (mut attempted, mut answered) = (0u32, 0u32);
        for i in due {
            let (class, group) = (self.schedule[i].class.clone(), self.schedule[i].group);
            let binding = self.def.classes[&class].clone();
            let channels = self.channels(&class).unwrap_or(1);
            for ch in 1..=channels {
                for name in &binding.poll[group].read {
                    let per_channel = uses_ch(&self.def, &self.def.commands[name]);
                    let v = if ch > 1 && !per_channel {
                        self.latest.get(&1).and_then(|r| r.get(name)).cloned().unwrap_or(Value::Null)
                    } else {
                        attempted += 1;
                        match self.query(name, ch) {
                            Ok(v) => {
                                answered += 1;
                                v
                            }
                            Err(Error::Io(m)) => return Err(Error::Io(m)),
                            Err(_) => Value::Null,
                        }
                    };
                    let settling = self.settle_until.is_some_and(|t| Instant::now() < t);
                    self.latest.entry(ch).or_default().insert(name.clone(), if settling { Value::Null } else { v });
                }
                let results = self.latest.get(&ch).cloned().unwrap_or_default();
                let status = self.compiled[&class].evaluate(&results, &self.state, ch);
                updates.push(StatusUpdate { class: class.clone(), ch, status });
            }
            self.schedule[i].next = Instant::now() + ms(binding.poll[group].every_ms);
        }
        if attempted > 0 {
            self.silent_polls = if answered == 0 { self.silent_polls + 1 } else { 0 };
        }
        Ok(updates)
    }

    /// Query a command (cached per `cache_ms`).
    pub fn query(&mut self, name: &str, ch: u8) -> Result<Value, Error> {
        let cmd = self.command(name)?;
        if let (Some(ttl), Some((v, at))) = (cmd.cache_ms, self.cache.get(&(name.to_string(), ch))) {
            if at.elapsed() < ms(ttl) {
                return Ok(v.clone());
            }
        }
        Ok(self.run(name, ch, &BTreeMap::new())?.value())
    }

    fn command(&self, name: &str) -> Result<Command, Error> {
        self.def.commands.get(name).cloned().ok_or_else(|| Error::Definition(format!("unknown command {name}")))
    }

    // ---- command execution -----------------------------------------------------------

    fn run_step(&mut self, step: &Step, ch: u8, value: Option<&str>) -> Result<Output, Error> {
        let cmd = self.command(step.command())?;
        let mut args = BTreeMap::new();
        match step {
            Step::Name(_) => {
                let caller: Vec<&String> = cmd.params.iter().filter(|(_, p)| p.from.is_none()).map(|(k, _)| k).collect();
                match (caller.as_slice(), value) {
                    ([], _) => {}
                    ([p], Some(v)) => {
                        args.insert((*p).clone(), v.to_string());
                    }
                    _ => return Err(Error::BadArgument(format!("`{}` needs a value", step.command()))),
                }
            }
            Step::Call { args: a, only, .. } => {
                if let (Some(only), Some(v)) = (only, value) {
                    if !only.iter().any(|o| o == v) {
                        return Ok(Output::Value(Value::Null));
                    }
                }
                for (k, v) in a {
                    let v = match (v.contains("{value}"), value) {
                        (false, _) => v.clone(),
                        (true, Some(val)) => v.replace("{value}", val),
                        (true, None) => return Err(Error::BadArgument("missing value".into())),
                    };
                    args.insert(k.clone(), v);
                }
            }
        }
        self.run(step.command(), ch, &args)
    }

    fn run(&mut self, name: &str, ch: u8, args: &BTreeMap<String, String>) -> Result<Output, Error> {
        let cmd = self.command(name)?;
        // Check the precondition first: a refused command sends nothing, not even its
        // `before` steps (a DHO804 logs "Command execute failed" when a hidden channel is
        // selected as waveform source).
        if let Some(req) = &cmd.requires {
            if self.eval_with_queries(&req.expr, ch)? != Value::Bool(true) {
                return Err(Error::Refused(req.message.clone()));
            }
        }
        for s in &cmd.before {
            self.run_step(s, ch, None)?;
        }
        let mut computed = BTreeMap::new();
        for spec in cmd.params.values() {
            if let Some(expr) = &spec.from {
                computed.insert(expr.clone(), self.eval_with_queries(expr, ch)?);
            }
        }
        let reply = match &cmd.modbus {
            Some(op) => self.modbus_exchange(&cmd, op, args)?,
            None => {
                let wire = render(&cmd, &self.def.framing.tx.terminator, args, ch, &self.state, &mut |e| {
                    computed.get(e).cloned().ok_or_else(|| Error::Definition(format!("uncomputed `{e}`")))
                })?;
                self.exchange(&cmd, wire.as_bytes())?
            }
        };
        let out = match reply {
            None => Output::Value(Value::Null),
            // a block without parse steps is returned as-is (images); with steps it is data
            Some(bytes) if cmd.reply == Reply::Block && cmd.parse.is_empty() => Output::Bytes(bytes),
            Some(bytes) => match parse_reply(&cmd, &bytes) {
                Ok(v) => Output::Value(v),
                Err(e) => {
                    // A reply of the wrong shape usually means the stream is out of step.
                    self.drain()?;
                    return Err(e);
                }
            },
        };
        if let (Some(_), Output::Value(v)) = (cmd.cache_ms, &out) {
            self.cache.insert((name.to_string(), ch), (v.clone(), Instant::now()));
        }
        for inv in &cmd.invalidates {
            self.cache.retain(|(n, _), _| n != inv);
        }
        for (var, tpl) in &cmd.effects {
            self.state.insert(var.clone(), Value::String(fill(tpl, args, ch)));
        }
        if let Some(s) = cmd.settle_ms {
            self.settle_until = Some(Instant::now() + ms(s));
        }
        Ok(out)
    }

    /// Evaluate a CEL expression, querying (or using cached results of) the commands it references.
    fn eval_with_queries(&mut self, src: &str, ch: u8) -> Result<Value, Error> {
        let expr = Expr::compile(src)?;
        let refs: Vec<String> = cel::Program::compile(src)
            .map(|p| p.references().variables().into_iter().map(|v| v.to_string()).collect())
            .unwrap_or_default();
        let names: Vec<String> = refs.into_iter().filter(|r| self.def.commands.contains_key(r)).collect();
        let mut results = BTreeMap::new();
        for r in names {
            let v = self.query(&r, ch)?;
            results.insert(r, v);
        }
        expr.eval(Vars { results: &results, state: &self.state, ch })
    }

    /// One exchange, counted for metrics.
    fn exchange(&mut self, cmd: &Command, wire: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        let started = Instant::now();
        let r = self.exchange_inner(cmd, wire);
        self.stats.exchanges += 1;
        self.stats.busy += started.elapsed();
        if r.is_err() {
            self.stats.failures += 1;
        }
        self.broken |= matches!(r, Err(Error::Io(_)));
        r
    }

    /// Respect gaps, write, frame the reply (or none).
    fn exchange_inner(&mut self, cmd: &Command, wire: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        let now = Instant::now();
        if now < self.next_write {
            sleep(self.next_write - now);
        }
        self.t.write(wire).map_err(|e| Error::Io(e.to_string()))?;
        let timing = self.def.framing.timing.clone();
        if cmd.reply == Reply::None {
            self.next_write = Instant::now() + ms(cmd.gap_after_ms.unwrap_or(timing.gap_after_write_ms));
            return Ok(None);
        }
        let rx = self.def.framing.rx.clone();
        let deadline = Instant::now() + ms(cmd.timeout_ms.unwrap_or(rx.timeout_ms));
        let idle_line = rx.mode == RxMode::Idle && cmd.reply == Reply::Line;
        let idle = ms(rx.idle_ms.unwrap_or(10));
        let mut framer = Framer::new(&rx, &cmd.reply);
        let result = loop {
            let now = Instant::now();
            if now >= deadline {
                break framer.finish().map(|f| f.reply);
            }
            let wait = if idle_line && !framer.buffered().is_empty() { idle.min(deadline - now) } else { deadline - now };
            let chunk = self.t.read(wait).map_err(|e| Error::Io(e.to_string()))?;
            if chunk.is_empty() {
                if idle_line && !framer.buffered().is_empty() {
                    break framer.finish().map(|f| f.reply);
                }
                continue;
            }
            match framer.push(&chunk) {
                Ok(Some(f)) => {
                    if !f.leftover.is_empty() {
                        self.drain()?;
                    }
                    break Ok(f.reply);
                }
                Ok(None) => {}
                Err(e) => break Err(e),
            }
        };
        let gap = cmd.gap_after_ms.unwrap_or(timing.min_gap_ms);
        match result {
            Ok(r) => {
                self.next_write = Instant::now() + ms(gap);
                Ok(Some(r))
            }
            Err(e) => {
                self.drain()?;
                self.next_write = Instant::now() + ms(gap);
                Err(e)
            }
        }
    }

    /// One Modbus RTU request: the frame from the op (+ the write's value param), the reply framed
    /// by its expected length and checked (address, function, CRC, exceptions). Reads return
    /// the data bytes, writes `None`.
    fn modbus_exchange(&mut self, cmd: &Command, op: &bm_devicedef::ModbusOp, args: &BTreeMap<String, String>) -> Result<Option<Vec<u8>>, Error> {
        let address = self.def.framing.modbus.as_ref().map(|m| m.address).ok_or_else(|| Error::Definition("modbus command without framing.modbus".into()))?;
        let value = match &op.value {
            None => None,
            Some(p) => {
                let spec = cmd.params.get(p).ok_or_else(|| Error::Definition(format!("modbus value param `{p}` is not declared")))?;
                let raw = args.get(p).ok_or_else(|| Error::BadArgument(format!("missing value for `{p}`")))?;
                let text = crate::wire::format_param(p, spec, raw)?;
                Some(text.trim().parse::<f64>().map_err(|_| Error::Definition(format!("`{p}` does not give a number for a register ({text:?})")))?)
            }
        };
        let frame = crate::modbus::request(address, op, value)?;
        let started = Instant::now();
        let r = self.modbus_exchange_inner(cmd, op, address, &frame);
        self.stats.exchanges += 1;
        self.stats.busy += started.elapsed();
        if r.is_err() {
            self.stats.failures += 1;
        }
        self.broken |= matches!(r, Err(Error::Io(_)));
        r
    }

    fn modbus_exchange_inner(&mut self, cmd: &Command, op: &bm_devicedef::ModbusOp, address: u8, frame: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        let now = Instant::now();
        if now < self.next_write {
            sleep(self.next_write - now);
        }
        self.t.write(frame).map_err(|e| Error::Io(e.to_string()))?;
        let timing = self.def.framing.timing.clone();
        let deadline = Instant::now() + ms(cmd.timeout_ms.unwrap_or(self.def.framing.rx.timeout_ms));
        let mut buf = Vec::new();
        let result = loop {
            let now = Instant::now();
            if now >= deadline {
                break Err(Error::Timeout { partial: buf.clone() });
            }
            let chunk = self.t.read(deadline - now).map_err(|e| Error::Io(e.to_string()))?;
            buf.extend_from_slice(&chunk);
            match crate::modbus::check_reply(address, op, &buf) {
                Ok(None) => {}
                Ok(Some(data)) => break Ok(data),
                Err(e) => break Err(e),
            }
        };
        let gap = cmd.gap_after_ms.unwrap_or(timing.min_gap_ms);
        self.next_write = Instant::now() + ms(gap);
        match result {
            Ok(data) if crate::modbus::is_read(op.function) => Ok(Some(data)),
            Ok(_) => Ok(None),
            Err(e) => {
                if !matches!(e, Error::Refused(_)) {
                    self.drain()?;
                }
                Err(e)
            }
        }
    }

    /// `text` as the engine sends a command: the definition's tx terminator appended.
    pub fn tx_frame(&self, text: &str) -> Vec<u8> {
        format!("{text}{}", self.def.framing.tx.terminator).into_bytes()
    }

    /// Diagnostics: write `wire` as it is (after the definition's gap) and keep everything that
    /// comes back, without framing: until `listen` ran out, or the line was quiet for `idle`
    /// once something arrived. Shows a reply that comes too late, ends differently than the
    /// definition expects or brings more than one frame. Nothing is parsed or cached.
    pub fn raw(&mut self, wire: &[u8], listen: Duration, idle: Duration) -> Result<RawReply, Error> {
        let r = self.raw_inner(wire, listen, idle);
        if matches!(r, Err(Error::Io(_))) {
            self.broken = true;
        }
        r
    }

    fn raw_inner(&mut self, wire: &[u8], listen: Duration, idle: Duration) -> Result<RawReply, Error> {
        let now = Instant::now();
        if now < self.next_write {
            sleep(self.next_write - now);
        }
        let stale = self.take_pending(Duration::from_millis(10))?;
        self.t.write(wire).map_err(|e| Error::Io(e.to_string()))?;
        let sent = Instant::now();
        let deadline = sent + listen;
        let mut out = RawReply { stale, ..RawReply::default() };
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let wait = if out.bytes.is_empty() { deadline - now } else { idle.min(deadline - now) };
            let chunk = self.t.read(wait).map_err(|e| Error::Io(e.to_string()))?;
            // a message link answers a query with one message (or nothing within the listen):
            // asking again would request one nobody queried
            let once = self.t.reads_on_request();
            if chunk.is_empty() {
                if !out.bytes.is_empty() || once {
                    break;
                }
                continue;
            }
            let at = sent.elapsed().as_millis() as u64;
            out.first_ms.get_or_insert(at);
            out.last_ms = Some(at);
            out.bytes.extend_from_slice(&chunk);
            if once {
                break;
            }
        }
        self.next_write = Instant::now() + ms(self.def.framing.timing.min_gap_ms);
        Ok(out)
    }

    /// Diagnostics: take what is waiting on the line (until it was quiet for `quiet`, at most
    /// 2 s): bytes nothing asked for now - a reply that came after its command's timeout, a
    /// message the instrument sent by itself. Taken, they cannot shift onto the next reply.
    pub fn take_pending(&mut self, quiet: Duration) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        // a message link has nothing waiting that a read could take without asking for it
        if self.t.reads_on_request() {
            return Ok(out);
        }
        let limit = Instant::now() + Duration::from_secs(2);
        while Instant::now() < limit {
            let chunk = self.t.read(quiet).map_err(|e| {
                self.broken = true;
                Error::Io(e.to_string())
            })?;
            if chunk.is_empty() {
                break;
            }
            out.extend_from_slice(&chunk);
        }
        Ok(out)
    }

    /// An exchange lost the transport (an I/O error): the link has to be opened again.
    pub fn link_broken(&self) -> bool {
        self.broken
    }

    /// Discard input until the line has been idle for `drain_idle_ms`.
    fn drain(&mut self) -> Result<(), Error> {
        if let Some(d) = self.def.framing.rx.drain_idle_ms {
            let limit = Instant::now() + Duration::from_secs(2);
            while Instant::now() < limit {
                if self.t.read(ms(d)).map_err(|e| Error::Io(e.to_string()))?.is_empty() {
                    break;
                }
            }
        }
        Ok(())
    }
}

/// A command is per channel if its text uses `{ch}`, a computed param depends on `ch`
/// (`from: "ch == 1 ? '' : ':CH2'"`), or one of its `before` steps is per channel (e.g.
/// `:WAVeform:DATA?` after selecting `:WAVeform:SOURce CHANnel{ch}`).
fn mentions_ch(expr: &str) -> bool {
    let b = expr.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'.';
    expr.match_indices("ch").any(|(i, _)| (i == 0 || !word(b[i - 1])) && b.get(i + 2).is_none_or(|&c| !word(c)))
}

fn uses_ch(def: &DeviceDef, cmd: &Command) -> bool {
    fn go(def: &DeviceDef, cmd: &Command, depth: u8) -> bool {
        template::placeholders(&cmd.send).map(|p| p.iter().any(|x| x == "ch")).unwrap_or(false)
            || cmd.params.values().any(|p| p.from.as_deref().is_some_and(mentions_ch))
            || (depth < 8 && cmd.before.iter().any(|s| def.commands.get(s.command()).is_some_and(|c| go(def, c, depth + 1))))
    }
    go(def, cmd, 0)
}

/// Fill an `effects` template with the caller's API values and `{ch}`.
fn fill(tpl: &str, args: &BTreeMap<String, String>, ch: u8) -> String {
    template::parse(tpl)
        .map(|pieces| {
            pieces
                .into_iter()
                .map(|p| match p {
                    Piece::Literal(l) => l,
                    Piece::Placeholder(n) if n == "ch" => ch.to_string(),
                    Piece::Placeholder(n) => args.get(&n).cloned().unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_else(|_| tpl.to_string())
}
