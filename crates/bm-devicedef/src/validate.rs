//! Semantic validation beyond what serde enforces: cross references, templates,
//! framing consistency, regexes, CEL expressions and class-contract coverage.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

static NUMBER_FORMAT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^%((\.\d+)?[fg]|0?\d*d)$").unwrap());
static SERIAL_FORMAT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[5-8][NEOMS](1|1\.5|2)$").unwrap());
static BIT_SPEC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)(\.\.(\d+))?$").unwrap());

use crate::model::*;
use crate::template;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    /// Location inside the document, e.g. `commands.vset.params.v`.
    pub path: String,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{s}: {}: {}", self.path, self.message)
    }
}

struct Ctx {
    issues: Vec<Issue>,
}

impl Ctx {
    fn err(&mut self, path: impl Into<String>, msg: impl Into<String>) {
        self.issues.push(Issue { severity: Severity::Error, path: path.into(), message: msg.into() });
    }
    fn warn(&mut self, path: impl Into<String>, msg: impl Into<String>) {
        self.issues.push(Issue { severity: Severity::Warning, path: path.into(), message: msg.into() });
    }
}

/// CEL variables available to `requires` and computed params: every query command that
/// takes no caller-supplied params (its cached/fresh result), plus `state` and `ch`.
fn query_vars(d: &DeviceDef) -> BTreeSet<String> {
    let mut vars: BTreeSet<String> = ["ch".to_string(), "state".to_string()].into();
    for (name, c) in &d.commands {
        let caller_params = c.params.values().any(|p| p.from.is_none());
        if c.reply != Reply::None && c.safety == Safety::Query && !caller_params {
            vars.insert(name.clone());
        }
    }
    vars
}

fn expected_repr(kind: ValueKind) -> Repr {
    match kind {
        ValueKind::Si => Repr::Si,
        ValueKind::Bytes => Repr::Bytes,
        _ => Repr::Raw,
    }
}

/// The parse steps leave a number (the last step that decides the kind is a numeric one).
fn parses_to_number(steps: &[ParseStep]) -> bool {
    steps
        .iter()
        .rev()
        .find(|s| !matches!(s, ParseStep::NullIf(_)))
        .is_some_and(|s| matches!(s, ParseStep::Number | ParseStep::Scale(_) | ParseStep::Byte))
}

fn number_format_ok(f: &str) -> bool {
    NUMBER_FORMAT.is_match(f)
}

pub fn validate_class(c: &ClassDef) -> Vec<Issue> {
    let mut cx = Ctx { issues: Vec::new() };
    if c.schema != CLASS_SCHEMA_ID {
        cx.err("schema", format!("expected `{CLASS_SCHEMA_ID}`, got `{}`", c.schema));
    }
    if c.class.len() != 3 || !c.class.chars().all(|ch| ch.is_ascii_uppercase()) {
        cx.err("class", "class code must be 3 uppercase letters");
    }
    for (k, s) in &c.status {
        if s.kind == ValueKind::Enum && s.values.is_empty() {
            cx.err(format!("status.{k}"), "enum status needs `values`");
        }
    }
    for (k, o) in &c.operations {
        if !o.get && !o.set {
            cx.err(format!("operations.{k}"), "operation must allow get and/or set");
        }
        if o.value == ValueKind::Enum && o.values.is_empty() {
            cx.err(format!("operations.{k}"), "enum operation needs `values`");
        }
        for dir in &o.required {
            let allowed = match dir {
                Direction::Get => o.get,
                Direction::Set => o.set,
            };
            if !allowed {
                cx.err(format!("operations.{k}.required"), format!("{dir:?} is required but not allowed").to_lowercase());
            }
        }
    }
    cx.issues
}

pub fn validate_device(d: &DeviceDef, classes: &BTreeMap<String, ClassDef>) -> Vec<Issue> {
    let mut cx = Ctx { issues: Vec::new() };
    if d.schema != DEVICE_SCHEMA_ID {
        cx.err("schema", format!("expected `{DEVICE_SCHEMA_ID}`, got `{}`", d.schema));
    }
    if d.models.is_empty() {
        cx.err("models", "at least one model is required");
    }
    for (i, m) in d.models.iter().enumerate() {
        if let Err(e) = Regex::new(&m.pattern) {
            cx.err(format!("models[{i}].match"), format!("invalid regex: {e}"));
        }
        for (class, n) in &m.channels {
            if !d.classes.contains_key(class) {
                cx.err(format!("models[{i}].channels.{class}"), "class not bound by this definition");
            }
            if *n == 0 {
                cx.err(format!("models[{i}].channels.{class}"), "channels must be >= 1");
            }
        }
        for class in m.features.keys() {
            if !d.classes.contains_key(class) {
                cx.err(format!("models[{i}].features.{class}"), "class not bound by this definition");
            }
        }
    }

    check_link_and_framing(d, &mut cx);

    match d.commands.get(&d.identify) {
        None => cx.err("identify", format!("unknown command `{}`", d.identify)),
        Some(c) if c.reply == Reply::None => cx.err("identify", "identify command must have a reply"),
        _ => {}
    }

    for (name, cmd) in &d.commands {
        check_command(d, name, cmd, &mut cx);
    }
    for (i, s) in d.session.on_connect.iter().enumerate() {
        check_step(d, &format!("session.on_connect[{i}]"), s, false, &mut cx);
    }
    for (i, s) in d.session.on_disconnect.iter().enumerate() {
        check_step(d, &format!("session.on_disconnect[{i}]"), s, false, &mut cx);
    }
    if d.classes.is_empty() {
        cx.err("classes", "a definition must bind at least one class");
    }
    for (class, b) in &d.classes {
        check_binding(d, class, b, classes.get(class), &mut cx);
    }

    // Commands nobody uses are usually a mistake (or API-only extras: warn only).
    let mut used: BTreeSet<&str> = BTreeSet::new();
    used.insert(d.identify.as_str());
    for s in d.session.on_connect.iter().chain(&d.session.on_disconnect) {
        used.insert(s.command());
    }
    let mut expr_refs: BTreeSet<String> = BTreeSet::new();
    for c in d.commands.values() {
        for s in &c.before {
            used.insert(s.command());
        }
        let exprs = c.requires.iter().map(|r| r.expr.as_str()).chain(c.params.values().filter_map(|p| p.from.as_deref()));
        for e in exprs {
            if let Ok(p) = cel::Program::compile(e) {
                expr_refs.extend(p.references().variables().into_iter().map(|v| v.to_string()));
            }
        }
    }
    for r in &expr_refs {
        if let Some((k, _)) = d.commands.get_key_value(r) {
            used.insert(k.as_str());
        }
    }
    for b in d.classes.values() {
        for g in &b.poll {
            used.extend(g.read.iter().map(String::as_str));
        }
        for op in b.operations.values() {
            for a in op.get.iter().chain(op.set.iter()) {
                for s in a.steps() {
                    used.insert(match &s {
                        Step::Name(n) => d.commands.get_key_value(n).map(|(k, _)| k.as_str()).unwrap_or(""),
                        Step::Call { cmd, .. } => d.commands.get_key_value(cmd).map(|(k, _)| k.as_str()).unwrap_or(""),
                    });
                }
            }
        }
    }
    for name in d.commands.keys() {
        if !used.contains(name.as_str()) {
            cx.warn(format!("commands.{name}"), "command is not used by identify, session, poll or any operation");
        }
    }
    cx.issues
}

fn check_link_and_framing(d: &DeviceDef, cx: &mut Ctx) {
    let links = d.link.all();
    if links.is_empty() {
        cx.err("link", "at least one link (serial, usbtmc, tcp, vxi11) is required");
    }
    if let Some(s) = &d.link.serial {
        if !SERIAL_FORMAT.is_match(&s.format) {
            cx.err("link.serial.format", format!("invalid serial format `{}` (e.g. 8N1)", s.format));
        }
        if s.baud == 0 {
            cx.err("link.serial.baud", "baud must be > 0");
        }
    }
    // the top-level framing serves every link without its own
    let shared: Vec<&Link> = links.iter().filter(|l| l.own_framing().is_none()).collect();
    let shared_usbtmc = !shared.is_empty() && shared.iter().all(|l| matches!(l, Link::Usbtmc(_) | Link::Vxi11(_)));
    check_framing(&d.framing, "framing", shared_usbtmc, d, cx);
    let own = [
        ("link.serial.framing", d.link.serial.as_ref().and_then(|l| l.framing.as_ref()), false),
        ("link.usbtmc.framing", d.link.usbtmc.as_ref().and_then(|l| l.framing.as_ref()), true),
        ("link.tcp.framing", d.link.tcp.as_ref().and_then(|l| l.framing.as_ref()), false),
        ("link.vxi11.framing", d.link.vxi11.as_ref().and_then(|l| l.framing.as_ref()), true),
    ];
    for (at, f, usbtmc) in own {
        if let Some(f) = f {
            check_framing(f, at, usbtmc, d, cx);
        }
    }
}

/// `usbtmc`: the framing is used on a USBTMC link (where `message` mode belongs).
fn check_framing(f: &Framing, at: &str, usbtmc: bool, d: &DeviceDef, cx: &mut Ctx) {
    let rx = &f.rx;
    match rx.mode {
        RxMode::Terminator => {
            if rx.terminator.as_deref().unwrap_or("").is_empty() {
                cx.err(format!("{at}.rx.terminator"), "mode `terminator` requires a non-empty terminator");
            }
        }
        RxMode::Idle => {
            if rx.idle_ms.is_none() {
                cx.err(format!("{at}.rx.idle_ms"), "mode `idle` requires idle_ms");
            }
        }
        RxMode::Fixed => {
            for (name, c) in &d.commands {
                if c.reply == Reply::Line {
                    cx.err(
                        format!("commands.{name}.reply"),
                        "rx mode `fixed` has no line framing: use `reply: {bytes: N}`, `none` or `block`",
                    );
                }
            }
        }
        RxMode::Message => {
            if !usbtmc {
                cx.warn(format!("{at}.rx.mode"), "mode `message` is meant for message-based links (usbtmc, vxi11)");
            }
        }
    }
    if rx.mode != RxMode::Terminator && rx.terminator.is_some() {
        cx.warn(format!("{at}.rx.terminator"), "terminator is ignored unless mode is `terminator`");
    }
    if rx.timeout_ms == 0 {
        cx.err(format!("{at}.rx.timeout_ms"), "timeout must be > 0");
    }
}

fn check_command(d: &DeviceDef, name: &str, cmd: &Command, cx: &mut Ctx) {
    let at = format!("commands.{name}");
    match &cmd.modbus {
        None if cmd.send.is_empty() => cx.err(format!("{at}.send"), "a command needs `send` (or `modbus`)"),
        None => {}
        Some(op) => {
            if !cmd.send.is_empty() {
                cx.err(format!("{at}.send"), "a `modbus` command has no `send`");
            }
            if d.framing.modbus.is_none() {
                cx.err(format!("{at}.modbus"), "modbus commands need `framing.modbus` (slave address)");
            }
            let read = matches!(op.function, ModbusFn::ReadCoils | ModbusFn::ReadDiscrete | ModbusFn::ReadHolding | ModbusFn::ReadInput);
            match (&op.value, read) {
                (Some(_), true) => cx.err(format!("{at}.modbus.value"), "a read takes no value"),
                (None, false) => cx.err(format!("{at}.modbus.value"), "a write needs `value` (the param holding it)"),
                (Some(v), false) if !cmd.params.contains_key(v) => cx.err(format!("{at}.modbus.value"), format!("`{v}` is not a param")),
                _ => {}
            }
            if op.function == ModbusFn::WriteSingle && op.encoding.words() != 1 {
                cx.err(format!("{at}.modbus.encoding"), "write_single writes one 16-bit register (use write_multiple)");
            }
            if read && cmd.parse.is_empty() {
                cx.warn(format!("{at}.parse"), "decode the register data with `word`, `registers` or `bits`");
            }
        }
    }
    match template::placeholders(&cmd.send) {
        Err(e) => cx.err(format!("{at}.send"), e),
        Ok(ph) => {
            let mut used = BTreeSet::new();
            for p in &ph {
                if p == "ch" {
                    continue;
                }
                if let Some(var) = p.strip_prefix("state.") {
                    if !d.state.contains_key(var) {
                        cx.err(format!("{at}.send"), format!("unknown state variable `{var}`"));
                    }
                    continue;
                }
                if cmd.params.contains_key(p) {
                    used.insert(p.clone());
                } else {
                    cx.err(format!("{at}.send"), format!("placeholder `{{{p}}}` is not a param, `ch` or `state.*`"));
                }
            }
            if let Some(op) = &cmd.modbus {
                if let Some(v) = &op.value {
                    used.insert(v.clone());
                }
            }
            for (p, spec) in &cmd.params {
                if !used.contains(p) && spec.from.is_none() {
                    let what = if cmd.modbus.is_some() { "`modbus.value`" } else { "`send`" };
                    cx.err(format!("{at}.params.{p}"), format!("param is not used in {what}"));
                }
            }
        }
    }
    for (p, spec) in &cmd.params {
        let pat = format!("{at}.params.{p}");
        match spec.kind {
            ValueKind::Enum if spec.values.is_empty() => cx.err(&pat, "enum param needs `values`"),
            ValueKind::Si | ValueKind::Bytes | ValueKind::List => cx.err(&pat, "`si`/`bytes`/`list` are class value kinds, not wire param kinds"),
            _ => {}
        }
        if let Some(expr) = &spec.from {
            let vars = query_vars(d);
            check_expr(expr, &vars, &format!("{pat}.from"), cx);
        }
        if let Some(f) = &spec.format {
            if !matches!(spec.kind, ValueKind::Number | ValueKind::Integer) {
                cx.err(&pat, "`format` applies to number/integer params only");
            } else if !number_format_ok(f) {
                cx.err(&pat, format!("unsupported format `{f}` (use %.Nf, %d or %g)"));
            }
        }
        if let (Some(lo), Some(hi)) = (spec.min, spec.max) {
            if lo > hi {
                cx.err(&pat, "min > max");
            }
        }
        if let Some(re) = &spec.pattern {
            if let Err(e) = Regex::new(re) {
                cx.err(&pat, format!("invalid pattern: {e}"));
            }
        }
    }
    if cmd.reply == Reply::None && !cmd.parse.is_empty() {
        cx.err(format!("{at}.parse"), "command has `reply: none` but a parse pipeline");
    }
    if cmd.reply == Reply::None && cmd.safety == Safety::Query {
        cx.warn(format!("{at}.safety"), "write-only command marked `query`; use `configure` or `energize`");
    }
    if cmd.reply == Reply::None && cmd.cache_ms.is_some() {
        cx.err(format!("{at}.cache_ms"), "nothing to cache for `reply: none`");
    }
    if cmd.reply == Reply::None && cmd.on_empty == OnEmpty::Null {
        cx.err(format!("{at}.on_empty"), "`on_empty` is meaningless for `reply: none`");
    }
    if cmd.max_bytes.is_some() && cmd.reply != Reply::Block {
        cx.warn(format!("{at}.max_bytes"), "max_bytes only applies to `reply: block`");
    }
    if let Some(r) = &cmd.requires {
        let mut vars = query_vars(d);
        vars.remove(name);
        check_expr(&r.expr, &vars, &format!("{at}.requires"), cx);
        if r.message.trim().is_empty() {
            cx.err(format!("{at}.requires.message"), "message must explain the refusal");
        }
    }
    for (i, step) in cmd.parse.iter().enumerate() {
        let sp = format!("{at}.parse[{i}]");
        match step {
            ParseStep::Regex { pattern, .. } => {
                if let Err(e) = Regex::new(pattern) {
                    cx.err(sp, format!("invalid regex: {e}"));
                }
            }
            ParseStep::Bits(m) => {
                for (k, v) in m {
                    match BIT_SPEC.captures(v) {
                        None => cx.err(&sp, format!("bits.{k}: `{v}` is not `N` or `A..B`")),
                        Some(c) => {
                            let a: u32 = c[1].parse().unwrap_or(99);
                            let b: u32 = c.get(3).map(|m| m.as_str().parse().unwrap_or(99)).unwrap_or(a);
                            if a > 63 || b > 63 || a > b {
                                cx.err(&sp, format!("bits.{k}: invalid range `{v}`"));
                            }
                        }
                    }
                }
            }
            ParseStep::Fields { names, .. } if names.is_empty() => cx.err(sp, "fields needs names"),
            ParseStep::Map(m) if m.is_empty() => cx.err(sp, "map is empty"),
            _ => {}
        }
    }
    for inv in &cmd.invalidates {
        match d.commands.get(inv) {
            None => cx.err(format!("{at}.invalidates"), format!("unknown command `{inv}`")),
            Some(c) if c.cache_ms.is_none() => {
                cx.warn(format!("{at}.invalidates"), format!("`{inv}` is not cached; invalidation has no effect"))
            }
            _ => {}
        }
    }
    for var in cmd.effects.keys() {
        if !d.state.contains_key(var) {
            cx.err(format!("{at}.effects"), format!("unknown state variable `{var}`"));
        }
    }
    for (i, s) in cmd.before.iter().enumerate() {
        if s.command() == name {
            cx.err(format!("{at}.before[{i}]"), "command cannot run itself as a precondition");
        }
        check_step(d, &format!("{at}.before[{i}]"), s, true, cx);
    }
}

/// `value_ok`: the step may bind the operation's `{value}` (set actions, `before` of a setter).
fn check_step(d: &DeviceDef, at: &str, step: &Step, value_ok: bool, cx: &mut Ctx) -> bool {
    let Some(cmd) = d.commands.get(step.command()) else {
        cx.err(at, format!("unknown command `{}`", step.command()));
        return false;
    };
    let args: BTreeMap<String, String> = match step {
        Step::Name(_) => BTreeMap::new(),
        Step::Call { args, .. } => args.clone(),
    };
    let mut binds_value = false;
    for (k, v) in &args {
        match cmd.params.get(k) {
            None => cx.err(at, format!("`{}` has no param `{k}`", step.command())),
            Some(p) if p.from.is_some() => cx.err(at, format!("param `{k}` is computed (`from`), it cannot be passed")),
            _ => {}
        }
        if v.contains("{value}") {
            if value_ok {
                binds_value = true;
            } else {
                cx.err(at, "`{value}` is only available in set actions");
            }
        }
    }
    let missing: Vec<&String> =
        cmd.params.iter().filter(|(p, spec)| spec.from.is_none() && !args.contains_key(*p)).map(|(p, _)| p).collect();
    match (missing.len(), value_ok) {
        (0, _) => {}
        (1, true) => binds_value = true, // the single unbound param receives the value
        _ => cx.err(at, format!("missing args for `{}`: {:?}", step.command(), missing)),
    }
    binds_value
}

fn check_binding(d: &DeviceDef, class: &str, b: &ClassBinding, spec: Option<&ClassDef>, cx: &mut Ctx) {
    let at = format!("classes.{class}");
    let Some(spec) = spec else {
        cx.err(&at, format!("unknown class `{class}` (no devices/classes/{class}.yaml)"));
        return;
    };
    if b.channels == 0 {
        cx.err(format!("{at}.channels"), "channels must be >= 1");
    }
    if b.poll.is_empty() {
        cx.err(format!("{at}.poll"), "at least one poll group is required");
    }
    let mut vars: BTreeSet<String> = ["ch".to_string(), "state".to_string()].into();
    for (g, group) in b.poll.iter().enumerate() {
        let gp = format!("{at}.poll[{g}]");
        if group.every_ms == 0 {
            cx.err(format!("{gp}.every_ms"), "poll interval must be > 0");
        }
        for (i, r) in group.read.iter().enumerate() {
            match d.commands.get(r) {
                None => cx.err(format!("{gp}.read[{i}]"), format!("unknown command `{r}`")),
                Some(c) if c.reply == Reply::None => cx.err(format!("{gp}.read[{i}]"), format!("`{r}` has no reply")),
                Some(c) if c.safety != Safety::Query => {
                    cx.err(format!("{gp}.read[{i}]"), format!("`{r}` is not a query; polling must not change state"))
                }
                Some(c) if c.params.values().any(|p| p.from.is_none()) => {
                    cx.err(format!("{gp}.read[{i}]"), format!("`{r}` needs params; polls cannot supply them"))
                }
                _ => {
                    if !vars.insert(r.clone()) {
                        cx.warn(format!("{gp}.read[{i}]"), format!("`{r}` is polled more than once"));
                    }
                }
            }
        }
    }
    for (key, field) in &b.status {
        let fp = format!("{at}.status.{key}");
        check_expr(field.expr(), &vars, &fp, cx);
        match spec.status.get(key) {
            None => cx.warn(&fp, format!("key not in class {class} contract")),
            Some(s) => {
                let want = expected_repr(s.kind);
                if want != field.repr() {
                    cx.err(&fp, format!("class contract type is {:?}; set `as: {want:?}`", s.kind).to_lowercase());
                }
            }
        }
    }
    for (key, s) in &spec.status {
        if s.required && !b.status.contains_key(key) {
            cx.err(format!("{at}.status"), format!("required status key `{key}` missing"));
        }
    }
    for (op, o) in &b.operations {
        let op_at = format!("{at}.operations.{op}");
        let ospec = spec.operations.get(op);
        if ospec.is_none() {
            cx.warn(&op_at, format!("operation not in class {class} contract"));
        }
        if o.get.is_none() && o.set.is_none() {
            cx.err(&op_at, "operation needs get and/or set");
        }
        if let Some(g) = &o.get {
            let steps = g.steps();
            for (i, s) in steps.iter().enumerate() {
                check_step(d, &format!("{op_at}.get[{i}]"), s, false, cx);
            }
            if let Some(last) = steps.last().and_then(|s| d.commands.get(s.command())) {
                if last.reply == Reply::None {
                    cx.err(format!("{op_at}.get"), "last get step must return a reply");
                }
            }
            if let Some(os) = ospec {
                if !os.get {
                    cx.err(format!("{op_at}.get"), "class contract does not allow get");
                }
                if expected_repr(os.value) != o.repr {
                    cx.err(format!("{op_at}.as"), format!("class contract value is {:?}; set `as: {:?}`", os.value, expected_repr(os.value)).to_lowercase());
                }
                let text = matches!(os.value, ValueKind::String | ValueKind::Enum);
                if let Some(last) = steps.last().and_then(|s| d.commands.get(s.command())) {
                    if text && o.repr == Repr::Raw && parses_to_number(&last.parse) {
                        cx.err(
                            format!("{op_at}.get"),
                            format!("`{}` is parsed to a number; the class contract wants text ({}): end its parse with `text`, or map the values", steps.last().unwrap().command(), format!("{:?}", os.value).to_lowercase()),
                        );
                    }
                }
            }
        }
        if let Some(s) = &o.set {
            let mut binds = false;
            for (i, st) in s.steps().iter().enumerate() {
                binds |= check_step(d, &format!("{op_at}.set[{i}]"), st, true, cx);
            }
            if let Some(os) = ospec {
                if !os.set {
                    cx.err(format!("{op_at}.set"), "class contract does not allow set");
                }
            }
            if !binds {
                cx.warn(format!("{op_at}.set"), "no step receives the POSTed value");
            }
        }
    }
    for (op, os) in &spec.operations {
        for dir in &os.required {
            let bound = b.operations.get(op).map(|o| match dir {
                Direction::Get => o.get.is_some(),
                Direction::Set => o.set.is_some(),
            });
            if bound != Some(true) {
                cx.err(format!("{at}.operations"), format!("required operation `{op}` ({dir:?}) missing").to_lowercase());
            }
        }
    }
}

fn check_expr(expr: &str, vars: &BTreeSet<String>, at: &str, cx: &mut Ctx) {
    match cel::Program::compile(expr) {
        Err(e) => cx.err(at, format!("CEL parse error in `{expr}`: {e}")),
        Ok(p) => {
            for v in p.references().variables() {
                if !vars.contains(v) {
                    cx.err(at, format!("unknown variable `{v}` (poll results, `ch`, `state`)"));
                }
            }
        }
    }
}
