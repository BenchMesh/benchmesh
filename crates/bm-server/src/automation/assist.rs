//! The flow assistant: the user describes an automation, an LLM of their choice (their own
//! API key) assembles a flow document, the server validates it and lets the model correct
//! its own mistakes. The result is only ever loaded into the editor - never run.

use std::time::Duration;

use serde_json::{json, Value};

use super::flow;
use super::store::Settings;
use super::vocab::Vocab;

const TIMEOUT: Duration = Duration::from_secs(120);
/// Corrections the model may make after the first answer fails validation.
const RETRIES: usize = 2;

/// The flow format for models (the assistant's prompt and MCP `flow_format`).
pub const FLOW_REFERENCE: &str = r#"# BenchMesh flow format (benchmesh.flow/v1)

A flow is a JSON document:

{
  "schema": "benchmesh.flow/v1",
  "name": "short title",
  "description": "what it does, in one or two sentences",
  "params": [ {"name": "target", "value": 5.0, "unit": "V", "description": "…", "min": 0, "max": 30} ],
  "setup":   [ steps ],   // runs first
  "steps":   [ steps ],   // the main part
  "cleanup": [ steps ],   // ALWAYS runs: after success, failure, abort, or a `stop` step
  "timeout_s": 600        // optional bound for setup + steps
}

`params` are the settings the user tunes without editing steps; their names are variables.

Every instrument step names `device` (config id), `class` (PSU, DMM, ELL, AWG, OSC, LCR, SAL)
and `channel` (default 1).

Steps (field `do`):
- {"do":"set", device, class, channel, "op":"voltage", "value": 5.0}  or  "expr": "target + 0.1"
    Calls a setter of the class. The instrument must report the value back within 6 s, else the
    run fails. Values outside the setter's min/max are refused.
- {"do":"read", device, class, channel, "field":"VOUT", "into":"v"}
    Reads a live status field (always newer than the run's last `set`). Or "op":"voltage" for a
    query. Numbers become numbers, ON/OFF etc. strings.
- {"do":"wait", "seconds": 2}  or  "expr": "dwell"
- {"do":"let", "var":"p", "expr":"v * i"}
- {"do":"if", "condition":"v > target", "then":[steps], "else":[steps]}
- {"do":"repeat", "times": 10 | "while":"cond" | "until":"cond", "max_iterations": 1000, "steps":[steps]}
    `iteration` (1, 2, …) inside. `while` is tested before each pass, `until` after it.
    Unbounded loops stop at max_iterations (default 10000). A pass takes at least 0.1 s.
- {"do":"sweep", "var":"i", "from":0, "to":1, "step":0.1, "steps":[steps]}  or  "values":[…]
- {"do":"check", "condition":"v < 5.5", "message":"over-voltage: {v} V", "fail":"stop"|"warn"}
- {"do":"log", "message":"pass {iteration}: {v} V"}       ({name} shows a variable)
- {"do":"prompt", "message":"Connect the battery, then press Continue"}
- {"do":"record", "action":"start", "name":"…", "interval_s":1,
     "series":[{"device":"dmm-1","class":"DMM","channel":1,"field":"MEAS","label":"Battery V"}]}
  {"do":"record", "action":"stop"}       (a recording is stopped at the end of the run anyway)
- {"do":"stop", "message":"done"}         ends the main part successfully; cleanup runs

Expressions are CEL: + - * / %, == != < <= > >=, && || !, cond ? a : b, functions
min(a,b), max(a,b), abs(x), round(x, digits), num(x). All numbers are decimals. Variables:
params, `into`/`var` of earlier steps, `iteration` in loops, `elapsed` (seconds since start).
A variable must be set before a step uses it.

Safety: a set that can switch an output/input on ("energizes": true) requires the user to arm
the run. If the run fails or is aborted, every output it switched on is switched off again.

Example - hold 5 V at the DMM by moving the electronic load's current:
{
  "schema": "benchmesh.flow/v1", "name": "Hold 5 V with the load",
  "params": [{"name":"target","value":5.0,"unit":"V"}, {"name":"step","value":0.05,"unit":"A"}, {"name":"limit","value":2.0,"unit":"A"}],
  "setup": [
    {"do":"set","device":"load-1","class":"ELL","op":"curr","value":0},
    {"do":"set","device":"load-1","class":"ELL","op":"input","value":"ON"}
  ],
  "steps": [{"do":"repeat","times":300,"steps":[
    {"do":"read","device":"dmm-1","class":"DMM","field":"MEAS","into":"v"},
    {"do":"read","device":"load-1","class":"ELL","field":"IIN","into":"i"},
    {"do":"if","condition":"v > target","then":[
       {"do":"set","device":"load-1","class":"ELL","op":"curr","expr":"min(i + step, limit)"}],
     "else":[
       {"do":"set","device":"load-1","class":"ELL","op":"curr","expr":"max(i - step, 0)"}]},
    {"do":"log","message":"{v} V at {i} A"},
    {"do":"wait","seconds":1}
  ]}],
  "cleanup": [{"do":"set","device":"load-1","class":"ELL","op":"input","value":"OFF"}]
}
"#;

const RULES: &str = r#"You assemble BenchMesh automation flows for a user's lab bench.

Rules:
- Use ONLY the devices, classes, channels, fields and operations listed in the bench vocabulary
  below. Setter values must respect each operation's value spec (type, min, max, enum values).
- Put every number the user may want to tune (targets, limits, step sizes, durations) in
  `params`, with a unit and a short description.
- Every flow that switches an output or input on (an operation with "energizes": true) must
  switch it off again in `cleanup`. Set safe setpoints (e.g. 0 A, the requested voltage) BEFORE
  switching an output on.
- Bound every loop (`times`, or `max_iterations` with `while`/`until`) and add `wait`s so the
  instruments are not polled faster than needed (≥ 0.5 s between readings unless asked).
- Prefer `read` of status fields (live, cheap) over queries (`op`).
- Add `check` steps for safety limits the user mentions (over-voltage, over-current, …).
- If the request is ambiguous or would need a value you must guess and that could damage
  something (a voltage, a current), choose the conservative value, make it a param and say so
  in `explanation`. If an instrument the request needs is not on the bench, say so.
- Answer ONLY by calling the tool `submit_flow` with the complete flow and a short explanation
  (what it does, what to check before running, assumptions). Never put the flow in plain text.
"#;

fn tool_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "flow": {"type": "object", "description": "The complete benchmesh.flow/v1 document."},
            "explanation": {"type": "string", "description": "What the flow does, assumptions, what to check before running it."}
        },
        "required": ["flow", "explanation"]
    })
}

pub fn system_prompt(vocab: &Vocab) -> String {
    format!(
        "{RULES}\n{FLOW_REFERENCE}\n# Bench vocabulary (devices on this bench)\n\n{}\n",
        serde_json::to_string_pretty(vocab).unwrap_or_default()
    )
}

pub fn default_model(provider: &str) -> Option<&'static str> {
    match provider {
        "anthropic" => Some("claude-sonnet-5-5"),
        _ => None,
    }
}

/// The model in use: the user's choice for this provider, else the provider's default.
pub fn model(s: &Settings) -> Option<String> {
    s.models.get(&s.provider).filter(|m| !m.trim().is_empty()).cloned().or_else(|| default_model(&s.provider).map(str::to_string))
}

fn base_url(s: &Settings) -> Result<String, String> {
    if let Some(u) = s.base_urls.get(&s.provider).map(|u| u.trim()).filter(|u| !u.is_empty()) {
        return Ok(u.trim_end_matches('/').to_string());
    }
    Ok(match s.provider.as_str() {
        "anthropic" => "https://api.anthropic.com/v1",
        "openai" => "https://api.openai.com/v1",
        "openrouter" => "https://openrouter.ai/api/v1",
        "custom" => return Err("the custom provider needs a base URL (an OpenAI-compatible endpoint, e.g. http://localhost:11434/v1)".into()),
        p => return Err(format!("unknown provider {p:?}")),
    }
    .to_string())
}

fn key(s: &Settings) -> Result<Option<String>, String> {
    match s.keys.get(&s.provider).filter(|k| !k.trim().is_empty()) {
        Some(k) => Ok(Some(k.trim().to_string())),
        // a local endpoint may need no key
        None if s.provider == "custom" => Ok(None),
        None => Err(format!("no API key for {} - add one in Automation → Settings", s.provider)),
    }
}

fn client() -> Result<reqwest::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(concat!("BenchMesh/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

/// Provider error bodies: `{"error": {"message": …}}` (both APIs) or raw text.
fn provider_error(status: reqwest::StatusCode, body: &str) -> String {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string).or_else(|| v["error"].as_str().map(str::to_string)))
        .unwrap_or_else(|| body.chars().take(300).collect());
    match status.as_u16() {
        401 | 403 => format!("the provider refused the API key ({status}): {msg}"),
        429 => format!("rate limited or out of credit ({status}): {msg}"),
        _ => format!("provider error {status}: {msg}"),
    }
}

/// One request: system prompt + user message → the `submit_flow` tool input.
async fn complete(s: &Settings, system: &str, user: &str) -> Result<Value, String> {
    let model = model(s).ok_or_else(|| format!("choose a model for {} in Automation → Settings", s.provider))?;
    let base = base_url(s)?;
    let key = key(s)?;
    let http = client()?;
    let (req, anthropic) = if s.provider == "anthropic" {
        let body = json!({
            "model": model, "max_tokens": 16000, "system": system,
            "messages": [{"role": "user", "content": user}],
            "tools": [{"name": "submit_flow", "description": "Submit the assembled flow.", "input_schema": tool_schema()}],
            // `auto`, never a forced tool: current models (Claude Sonnet 5.5, Opus 5.5, Fable 5.1+)
            // refuse `tool` / `any` with a 400; the system prompt asks for `submit_flow` only
            "tool_choice": {"type": "auto"},
        });
        let r = http.post(format!("{base}/messages")).header("anthropic-version", "2023-06-01").header("x-api-key", key.unwrap_or_default()).json(&body);
        (r, true)
    } else {
        let body = json!({
            "model": model,
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
            "tools": [{"type": "function", "function": {"name": "submit_flow", "description": "Submit the assembled flow.", "parameters": tool_schema()}}],
            "tool_choice": {"type": "function", "function": {"name": "submit_flow"}},
        });
        let mut r = http.post(format!("{base}/chat/completions")).json(&body);
        if let Some(k) = key {
            r = r.bearer_auth(k);
        }
        if s.provider == "openrouter" {
            r = r.header("HTTP-Referer", "https://benchmesh.online").header("X-Title", "BenchMesh");
        }
        (r, false)
    };
    let resp = req.send().await.map_err(|e| format!("cannot reach the provider: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(provider_error(status, &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("unreadable provider reply: {e}"))?;
    let input = if anthropic {
        v["content"].as_array().and_then(|c| c.iter().find(|b| b["type"] == "tool_use" && b["name"] == "submit_flow")).map(|b| b["input"].clone())
    } else {
        let call = &v["choices"][0]["message"]["tool_calls"][0]["function"];
        match &call["arguments"] {
            Value::String(a) => serde_json::from_str(a).ok(),
            Value::Object(_) => Some(call["arguments"].clone()),
            _ => None,
        }
    };
    input.ok_or_else(|| no_flow(&v, anthropic))
}

/// Why an answer carries no `submit_flow` call: cut off, declined, or what the model said instead.
fn no_flow(v: &Value, anthropic: bool) -> String {
    let stop = if anthropic { v["stop_reason"].as_str() } else { v["choices"][0]["finish_reason"].as_str() };
    match stop {
        Some("max_tokens") | Some("length") => return "the model ran out of output tokens before submitting a flow - describe a smaller flow or split it".into(),
        Some("refusal") | Some("content_filter") => return "the provider declined this request".into(),
        _ => {}
    }
    let said = if anthropic {
        // the first text block: a thinking block may come before it
        v["content"].as_array().and_then(|c| c.iter().find(|b| b["type"] == "text")).and_then(|b| b["text"].as_str())
    } else {
        v["choices"][0]["message"]["content"].as_str()
    };
    format!("the model did not submit a flow{}", said.map(|t| format!(": {}", t.chars().take(400).collect::<String>())).unwrap_or_default())
}

/// The model's answer: the flow may arrive as an object or as a JSON string.
fn unpack(input: &Value) -> (Value, String) {
    let flow = match &input["flow"] {
        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        v => v.clone(),
    };
    (flow, input["explanation"].as_str().unwrap_or("").to_string())
}

#[derive(Debug)]
pub struct Assisted {
    pub flow: Value,
    pub explanation: String,
    pub issues: Vec<flow::Issue>,
    pub attempts: usize,
}

/// Assemble (or, with `current`, change) a flow from the user's description.
pub async fn assist(s: &Settings, vocab: &Vocab, instruction: &str, current: Option<&Value>) -> Result<Assisted, String> {
    let system = system_prompt(vocab);
    let mut user = match current {
        Some(f) => format!("The current flow:\n```json\n{}\n```\n\nChange it as follows (submit the whole updated flow):\n{instruction}", serde_json::to_string_pretty(f).unwrap_or_default()),
        None => format!("Assemble a flow for this request:\n{instruction}"),
    };
    let mut attempts = 0;
    loop {
        attempts += 1;
        let (flow_doc, explanation) = unpack(&complete(s, &system, &user).await?);
        let (_, issues, _) = flow::check(&flow_doc, vocab);
        if !flow::has_errors(&issues) || attempts > RETRIES {
            return Ok(Assisted { flow: flow_doc, explanation, issues, attempts });
        }
        let errors: Vec<String> = issues.iter().filter(|i| i.level == "error").map(|i| format!("- {}: {}", if i.path.is_empty() { "(flow)" } else { &i.path }, i.message)).collect();
        user = format!(
            "{user}\n\nYour previous flow:\n```json\n{}\n```\nfails validation:\n{}\n\nFix these problems and submit the complete corrected flow.",
            serde_json::to_string_pretty(&flow_doc).unwrap_or_default(),
            errors.join("\n")
        );
    }
}

/// The provider's models (`[{id, name}]`), for the settings' model picker.
pub async fn models(s: &Settings) -> Result<Vec<Value>, String> {
    let base = base_url(s)?;
    let http = client()?;
    let url = if s.provider == "anthropic" { format!("{base}/models?limit=100") } else { format!("{base}/models") };
    let mut r = http.get(url);
    if s.provider == "anthropic" {
        r = r.header("anthropic-version", "2023-06-01").header("x-api-key", key(s)?.unwrap_or_default());
    } else if let Some(k) = key(s).ok().flatten() {
        r = r.bearer_auth(k);
    }
    let resp = r.send().await.map_err(|e| format!("cannot reach the provider: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(provider_error(status, &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("unreadable provider reply: {e}"))?;
    let mut out: Vec<Value> = v["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?;
                    Some(json!({"id": id, "name": m["display_name"].as_str().or(m["name"].as_str()).unwrap_or(id)}))
                })
                .collect()
        })
        .unwrap_or_default();
    if s.provider != "anthropic" {
        out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::routing::{get, post};
    use axum::{Json, Router};

    use super::super::flow::tests::{hold_flow, vocab};
    use super::*;

    /// A fake provider answering with these tool inputs in turn; records the requests.
    async fn provider(answers: Vec<Value>, anthropic: bool) -> (String, Arc<Mutex<Vec<Value>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let answers = Arc::new(Mutex::new(answers));
        let (s2, a2) = (seen.clone(), answers.clone());
        let handler = move |Json(body): Json<Value>| {
            let (seen, answers) = (s2.clone(), a2.clone());
            async move {
                seen.lock().unwrap().push(body);
                let input = answers.lock().unwrap().remove(0);
                if anthropic {
                    Json(json!({"content": [{"type": "tool_use", "name": "submit_flow", "input": input}]}))
                } else {
                    Json(json!({"choices": [{"message": {"tool_calls": [{"function": {"name": "submit_flow", "arguments": input.to_string()}}]}}]}))
                }
            }
        };
        let app = Router::new()
            .route("/v1/messages", post(handler.clone()))
            .route("/v1/chat/completions", post(handler))
            .route("/v1/models", get(|| async { Json(json!({"data": [{"id": "m-2", "display_name": "Model Two"}, {"id": "m-1"}]})) }));
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        (url, seen)
    }

    fn settings(provider: &str, base: &str) -> Settings {
        let mut s = Settings { provider: provider.into(), ..Default::default() };
        s.base_urls.insert(provider.into(), base.into());
        s.keys.insert(provider.into(), "test-key".into());
        s.models.insert(provider.into(), "m-1".into());
        s
    }

    #[tokio::test]
    async fn a_valid_flow_comes_back_on_the_first_attempt() {
        let (url, seen) = provider(vec![json!({"flow": hold_flow(), "explanation": "holds 5 V"})], true).await;
        let r = assist(&settings("anthropic", &url), &vocab(), "hold 5 V with the load", None).await.unwrap();
        assert_eq!(r.attempts, 1);
        assert_eq!(r.explanation, "holds 5 V");
        assert!(!flow::has_errors(&r.issues));
        let req = &seen.lock().unwrap()[0];
        assert_eq!(req["tool_choice"], json!({"type": "auto"}), "current Claude models refuse a forced tool choice");
        assert_eq!(req["tools"][0]["name"], "submit_flow");
        let system = req["system"].as_str().unwrap();
        assert!(system.contains("benchmesh.flow/v1") && system.contains("\"load\""), "the prompt carries the format and the bench");
    }

    #[tokio::test]
    async fn validation_errors_go_back_to_the_model_which_corrects_them() {
        let mut bad = hold_flow();
        bad["setup"][0]["op"] = json!("current");
        let (url, seen) = provider(vec![json!({"flow": bad, "explanation": "first"}), json!({"flow": hold_flow().to_string(), "explanation": "fixed"})], false).await;
        let r = assist(&settings("openrouter", &url), &vocab(), "hold 5 V", None).await.unwrap();
        assert_eq!((r.attempts, r.explanation.as_str()), (2, "fixed"));
        let second = seen.lock().unwrap()[1]["messages"][1]["content"].as_str().unwrap().to_string();
        assert!(second.contains("setup[0]") && second.contains("no setter \"current\""), "{second}");
    }

    #[tokio::test]
    async fn missing_keys_and_models_are_explained() {
        let s = Settings { provider: "openai".into(), ..Default::default() };
        assert!(assist(&s, &vocab(), "x", None).await.unwrap_err().contains("model"));
        let mut s = Settings { provider: "openai".into(), ..Default::default() };
        s.models.insert("openai".into(), "gpt".into());
        assert!(assist(&s, &vocab(), "x", None).await.unwrap_err().contains("no API key"));
    }

    #[test]
    fn an_answer_without_a_flow_is_explained() {
        let thought = json!({"stop_reason": "end_turn", "content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": "Which load?"}]});
        assert_eq!(no_flow(&thought, true), "the model did not submit a flow: Which load?", "the text block, not the thinking before it");
        assert!(no_flow(&json!({"stop_reason": "max_tokens", "content": []}), true).contains("ran out of output tokens"));
        assert!(no_flow(&json!({"stop_reason": "refusal", "content": []}), true).contains("declined"));
        assert!(no_flow(&json!({"choices": [{"finish_reason": "length", "message": {}}]}), false).contains("ran out of output tokens"));
        assert_eq!(no_flow(&json!({"choices": [{"message": {"content": "hi"}}]}), false), "the model did not submit a flow: hi");
    }

    #[tokio::test]
    async fn the_model_list_comes_from_the_provider() {
        let (url, _) = provider(vec![], false).await;
        let m = models(&settings("openai", &url)).await.unwrap();
        assert_eq!(m, vec![json!({"id": "m-1", "name": "m-1"}), json!({"id": "m-2", "name": "Model Two"})]);
    }
}
