use crate::relay_config::RelaySettings;
use reqwest::blocking::Client;
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static ACTIVE_ROUTER: OnceLock<Mutex<Option<RouterConfig>>> = OnceLock::new();

#[derive(Clone, Debug)]
struct RouterConfig {
    port: u16,
    upstream_base_url: String,
    api_key: String,
    upstream_model: Option<String>,
    upstream_wire_api: String,
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

#[derive(Clone, Debug)]
struct ResponseStreamState {
    response_id: String,
    text_item_id: String,
    reasoning_item_id: String,
    model: String,
    created_at: i64,
    accumulated_text: String,
    accumulated_reasoning: String,
    text_started: bool,
    text_done: bool,
    reasoning_started: bool,
    reasoning_done: bool,
    finish_reason: Option<String>,
    usage: Option<Value>,
    tool_calls: BTreeMap<usize, StreamToolCallState>,
}

#[derive(Clone, Debug, Default)]
struct StreamToolCallState {
    output_index: Option<u32>,
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    done: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlineThinkMode {
    Text,
    Reasoning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlineDeltaKind {
    Text,
    Reasoning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InlineDelta {
    kind: InlineDeltaKind,
    text: String,
}

#[derive(Debug)]
struct InlineThinkParser {
    mode: InlineThinkMode,
    pending: String,
}

impl InlineThinkParser {
    fn new() -> Self {
        Self {
            mode: InlineThinkMode::Text,
            pending: String::new(),
        }
    }

    fn push(&mut self, delta: &str) -> Vec<InlineDelta> {
        if delta.is_empty() {
            return Vec::new();
        }
        self.pending.push_str(delta);
        self.drain(false)
    }

    fn finish(&mut self) -> Vec<InlineDelta> {
        self.drain(true)
    }

    fn drain(&mut self, flush: bool) -> Vec<InlineDelta> {
        let mut events = Vec::new();
        loop {
            match self.mode {
                InlineThinkMode::Text => {
                    if let Some(index) = self.pending.find("<think>") {
                        let before = self.pending[..index].to_string();
                        if !before.is_empty() {
                            events.push(InlineDelta {
                                kind: InlineDeltaKind::Text,
                                text: before,
                            });
                        }
                        self.pending = self.pending[index + "<think>".len()..].to_string();
                        self.mode = InlineThinkMode::Reasoning;
                        continue;
                    }

                    let keep = if flush {
                        0
                    } else {
                        partial_tag_suffix_len(&self.pending, "<think>")
                    };
                    let emit_len = self.pending.len().saturating_sub(keep);
                    if emit_len > 0 {
                        let text = self.pending[..emit_len].to_string();
                        self.pending = self.pending[emit_len..].to_string();
                        events.push(InlineDelta {
                            kind: InlineDeltaKind::Text,
                            text,
                        });
                    }
                    break;
                }
                InlineThinkMode::Reasoning => {
                    if let Some(index) = self.pending.find("</think>") {
                        let reasoning = self.pending[..index].to_string();
                        if !reasoning.is_empty() {
                            events.push(InlineDelta {
                                kind: InlineDeltaKind::Reasoning,
                                text: reasoning,
                            });
                        }
                        self.pending = self.pending[index + "</think>".len()..].to_string();
                        self.mode = InlineThinkMode::Text;
                        continue;
                    }

                    let keep = if flush {
                        0
                    } else {
                        partial_tag_suffix_len(&self.pending, "</think>")
                    };
                    let emit_len = self.pending.len().saturating_sub(keep);
                    if emit_len > 0 {
                        let reasoning = self.pending[..emit_len].to_string();
                        self.pending = self.pending[emit_len..].to_string();
                        events.push(InlineDelta {
                            kind: InlineDeltaKind::Reasoning,
                            text: reasoning,
                        });
                    }
                    break;
                }
            }
        }
        events
    }
}

impl ResponseStreamState {
    fn new(model: String) -> Self {
        let created_at = unix_timestamp();
        let stamp = format!("{created_at}{}", current_millis() % 100_000);
        Self {
            response_id: format!("resp_codex_toolkit_{stamp}"),
            text_item_id: format!("msg_codex_toolkit_{stamp}"),
            reasoning_item_id: format!("rs_codex_toolkit_{stamp}"),
            model,
            created_at,
            accumulated_text: String::new(),
            accumulated_reasoning: String::new(),
            text_started: false,
            text_done: false,
            reasoning_started: false,
            reasoning_done: false,
            finish_reason: None,
            usage: None,
            tool_calls: BTreeMap::new(),
        }
    }

    fn response_object(&self, status: &str) -> Value {
        let mut output_items: Vec<(u32, Value)> = Vec::new();
        if self.reasoning_started {
            output_items.push((
                self.reasoning_output_index(),
                json!({
                    "id": self.reasoning_item_id,
                    "type": "reasoning",
                    "summary": [{
                        "type": "summary_text",
                        "text": self.accumulated_reasoning
                    }]
                }),
            ));
        }
        if self.text_started || !self.accumulated_text.is_empty() {
            output_items.push((
                self.text_output_index(),
                json!({
                    "id": self.text_item_id,
                    "type": "message",
                    "status": status,
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": self.accumulated_text,
                        "annotations": []
                    }]
                }),
            ));
        }
        for tool in self.tool_calls.values() {
            if let Some(output_index) = tool.output_index {
                output_items.push((output_index, tool_call_response_item(tool)));
            }
        }
        output_items.sort_by_key(|(index, _)| *index);
        let output = output_items
            .into_iter()
            .map(|(_, item)| item)
            .collect::<Vec<_>>();

        let mut response = json!({
            "id": self.response_id,
            "object": "response",
            "created_at": self.created_at,
            "model": self.model,
            "status": status,
            "output": output
        });
        if status == "incomplete" {
            response["incomplete_details"] = json!({
                "reason": "max_output_tokens"
            });
        }
        if let Some(usage) = &self.usage {
            response["usage"] = usage.clone();
        }
        response
    }

    fn start_events(&self) -> Vec<(&'static str, Value)> {
        vec![
            (
                "response.created",
                json!({
                    "type": "response.created",
                    "response": self.response_object("in_progress")
                }),
            ),
            (
                "response.in_progress",
                json!({
                    "type": "response.in_progress",
                    "response": self.response_object("in_progress")
                }),
            ),
        ]
    }

    fn reasoning_delta_events(&mut self, delta: &str) -> Vec<(&'static str, Value)> {
        if delta.is_empty() {
            return Vec::new();
        }
        let mut events = Vec::new();
        if !self.reasoning_started {
            self.reasoning_started = true;
            events.push((
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": self.reasoning_output_index(),
                    "item": {
                        "id": self.reasoning_item_id,
                        "type": "reasoning",
                        "status": "in_progress",
                        "summary": []
                    }
                }),
            ));
            events.push((
                "response.reasoning_summary_part.added",
                json!({
                    "type": "response.reasoning_summary_part.added",
                    "item_id": self.reasoning_item_id,
                    "output_index": self.reasoning_output_index(),
                    "summary_index": 0,
                    "part": {
                        "type": "summary_text",
                        "text": ""
                    }
                }),
            ));
        }
        self.accumulated_reasoning.push_str(delta);
        events.push((
            "response.reasoning_summary_text.delta",
            json!({
                "type": "response.reasoning_summary_text.delta",
                "item_id": self.reasoning_item_id,
                "output_index": self.reasoning_output_index(),
                "summary_index": 0,
                "delta": delta
            }),
        ));
        events
    }

    fn delta_events(&mut self, delta: &str) -> Vec<(&'static str, Value)> {
        let mut events = Vec::new();
        if !self.text_started {
            self.text_started = true;
            events.push((
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": self.text_output_index(),
                    "item": {
                        "id": self.text_item_id,
                        "type": "message",
                        "status": "in_progress",
                        "role": "assistant",
                        "content": []
                    }
                }),
            ));
            events.push((
                "response.content_part.added",
                json!({
                    "type": "response.content_part.added",
                    "item_id": self.text_item_id,
                    "output_index": self.text_output_index(),
                    "content_index": 0,
                    "part": {
                        "type": "output_text",
                        "text": "",
                        "annotations": []
                    }
                }),
            ));
        }
        self.accumulated_text.push_str(delta);
        events.push((
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta",
                "item_id": self.text_item_id,
                "output_index": self.text_output_index(),
                "content_index": 0,
                "delta": delta
            }),
        ));
        events
    }

    fn done_events(&mut self) -> Vec<(&'static str, Value)> {
        let mut events = Vec::new();
        if self.reasoning_started && !self.reasoning_done {
            self.reasoning_done = true;
            events.extend([
                (
                    "response.reasoning_summary_text.done",
                    json!({
                        "type": "response.reasoning_summary_text.done",
                        "item_id": self.reasoning_item_id,
                        "output_index": self.reasoning_output_index(),
                        "summary_index": 0,
                        "text": self.accumulated_reasoning
                    }),
                ),
                (
                    "response.reasoning_summary_part.done",
                    json!({
                        "type": "response.reasoning_summary_part.done",
                        "item_id": self.reasoning_item_id,
                        "output_index": self.reasoning_output_index(),
                        "summary_index": 0,
                        "part": {
                            "type": "summary_text",
                            "text": self.accumulated_reasoning
                        }
                    }),
                ),
                (
                    "response.output_item.done",
                    json!({
                        "type": "response.output_item.done",
                        "output_index": self.reasoning_output_index(),
                        "item": {
                            "id": self.reasoning_item_id,
                            "type": "reasoning",
                            "summary": [{
                                "type": "summary_text",
                                "text": self.accumulated_reasoning
                            }]
                        }
                    }),
                ),
            ]);
        }

        if self.text_started && !self.text_done {
            self.text_done = true;
            events.extend([
                (
                    "response.output_text.done",
                    json!({
                        "type": "response.output_text.done",
                        "item_id": self.text_item_id,
                        "output_index": self.text_output_index(),
                        "content_index": 0,
                        "text": self.accumulated_text
                    }),
                ),
                (
                    "response.content_part.done",
                    json!({
                        "type": "response.content_part.done",
                        "item_id": self.text_item_id,
                        "output_index": self.text_output_index(),
                        "content_index": 0,
                        "part": {
                            "type": "output_text",
                            "text": self.accumulated_text,
                            "annotations": []
                        }
                    }),
                ),
                (
                    "response.output_item.done",
                    json!({
                        "type": "response.output_item.done",
                        "output_index": self.text_output_index(),
                        "item": {
                            "id": self.text_item_id,
                            "type": "message",
                            "status": "completed",
                            "role": "assistant",
                            "content": [{
                                "type": "output_text",
                                "text": self.accumulated_text,
                                "annotations": []
                            }]
                        }
                    }),
                ),
            ]);
        }

        let tool_indexes = self.tool_calls.keys().copied().collect::<Vec<_>>();
        for tool_index in tool_indexes {
            events.extend(self.tool_call_done_events(tool_index));
        }

        let status = self.response_status();
        events.push((
            "response.completed",
            json!({
                "type": "response.completed",
                "response": self.response_object(status)
            }),
        ));
        events
    }

    fn failed_events(&mut self, message: &str) -> Vec<(&'static str, Value)> {
        vec![(
            "response.failed",
            json!({
                "type": "response.failed",
                "response": self.response_object("failed"),
                "error": {
                    "message": message,
                    "type": "server_error",
                    "code": "upstream_stream_error"
                }
            }),
        )]
    }

    fn set_finish_reason(&mut self, finish_reason: Option<String>) {
        if finish_reason.is_some() {
            self.finish_reason = finish_reason;
        }
    }

    fn set_usage(&mut self, usage: Option<Value>) {
        if let Some(usage) = usage {
            self.usage = Some(normalize_usage(&usage));
        }
    }

    fn response_status(&self) -> &'static str {
        match self.finish_reason.as_deref() {
            Some("length") => "incomplete",
            _ => "completed",
        }
    }

    fn tool_call_delta_events(&mut self, tool_call: &Value) -> Vec<(&'static str, Value)> {
        let chat_index = tool_call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let output_index = self.ensure_tool_call(chat_index, tool_call);
        let Some(tool) = self.tool_calls.get_mut(&chat_index) else {
            return Vec::new();
        };

        let function = tool_call.get("function").unwrap_or(&Value::Null);
        if let Some(id) = tool_call.get("id").and_then(Value::as_str) {
            if !id.trim().is_empty() {
                tool.call_id = id.to_string();
            }
        }
        if let Some(name) = function.get("name").and_then(Value::as_str) {
            if tool.name.is_empty() {
                tool.name = name.to_string();
            } else if tool.name != name && !tool.name.ends_with(name) {
                tool.name.push_str(name);
            }
        }
        let arguments_delta = function
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !arguments_delta.is_empty() {
            tool.arguments.push_str(arguments_delta);
        }

        if arguments_delta.is_empty() {
            return Vec::new();
        }
        vec![(
            "response.function_call_arguments.delta",
            json!({
                "type": "response.function_call_arguments.delta",
                "item_id": tool.item_id,
                "output_index": output_index,
                "delta": arguments_delta
            }),
        )]
    }

    fn ensure_tool_call(&mut self, chat_index: usize, tool_call: &Value) -> u32 {
        if let Some(output_index) = self
            .tool_calls
            .get(&chat_index)
            .and_then(|tool| tool.output_index)
        {
            return output_index;
        }

        let output_index = self.next_output_index();
        let fallback_id = format!("call_{}_{}", self.response_id, chat_index);
        let call_id = tool_call
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&fallback_id)
            .to_string();
        let item_id = format!("fc_{}", call_id);
        let name = tool_call
            .get("function")
            .and_then(|function| function.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        self.tool_calls.insert(
            chat_index,
            StreamToolCallState {
                output_index: Some(output_index),
                item_id,
                call_id,
                name,
                arguments: String::new(),
                done: false,
            },
        );
        output_index
    }

    fn tool_call_added_events(&mut self, tool_call: &Value) -> Vec<(&'static str, Value)> {
        let chat_index = tool_call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let already_exists = self.tool_calls.contains_key(&chat_index);
        let output_index = self.ensure_tool_call(chat_index, tool_call);
        if already_exists {
            return Vec::new();
        }
        let tool = self.tool_calls.get(&chat_index).unwrap();
        vec![(
            "response.output_item.added",
            json!({
                "type": "response.output_item.added",
                "output_index": output_index,
                "item": {
                    "id": tool.item_id,
                    "type": "function_call",
                    "status": "in_progress",
                    "call_id": tool.call_id,
                    "name": tool.name,
                    "arguments": ""
                }
            }),
        )]
    }

    fn tool_call_done_events(&mut self, chat_index: usize) -> Vec<(&'static str, Value)> {
        let Some(tool) = self.tool_calls.get_mut(&chat_index) else {
            return Vec::new();
        };
        if tool.done {
            return Vec::new();
        }
        tool.done = true;
        let output_index = tool.output_index.unwrap_or(0);
        let item = tool_call_response_item(tool);
        vec![(
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": output_index,
                "item": item
            }),
        )]
    }

    fn next_output_index(&self) -> u32 {
        let mut next = 0;
        if self.reasoning_started {
            next += 1;
        }
        if self.text_started || !self.accumulated_text.is_empty() {
            next += 1;
        }
        next + self
            .tool_calls
            .values()
            .filter(|tool| tool.output_index.is_some())
            .count() as u32
    }

    fn reasoning_output_index(&self) -> u32 {
        0
    }

    fn text_output_index(&self) -> u32 {
        if self.reasoning_started {
            1
        } else {
            0
        }
    }
}

pub fn ensure_router_for_settings(settings: &RelaySettings) -> Result<(), String> {
    if !settings.enabled || settings.route_mode != "local_router" {
        log_router_message(&format!(
            "router skipped enabled={} route_mode={}",
            settings.enabled, settings.route_mode
        ));
        return Ok(());
    }

    let config = RouterConfig {
        port: settings.local_port,
        upstream_base_url: settings.base_url.trim().trim_end_matches('/').to_string(),
        api_key: settings.api_key.trim().to_string(),
        upstream_model: settings.upstream_model.clone(),
        upstream_wire_api: settings.upstream_wire_api.clone(),
    };

    if config.upstream_base_url.is_empty() {
        log_router_message("router failed: upstream base url is empty");
        return Err("API Base URL cannot be empty.".to_string());
    }
    if config.api_key.is_empty() {
        log_router_message("router failed: api key is empty");
        return Err("API Key cannot be empty.".to_string());
    }
    if config.upstream_wire_api != "chat_completions" {
        log_router_message(&format!(
            "router failed: unsupported upstream wire api {}",
            config.upstream_wire_api
        ));
        return Err("Local router currently supports chat_completions upstreams only.".to_string());
    }

    let state = ACTIVE_ROUTER.get_or_init(|| Mutex::new(None));
    let mut active = state
        .lock()
        .map_err(|_| "Local router is locked.".to_string())?;
    let should_start = active
        .as_ref()
        .map(|item| item.port != config.port)
        .unwrap_or(true);
    *active = Some(config.clone());
    drop(active);

    if should_start {
        log_router_message(&format!(
            "router starting port={} upstream={} model={}",
            config.port,
            config.upstream_base_url,
            config.upstream_model.as_deref().unwrap_or("")
        ));
        start_listener(config.port)?;
    } else {
        log_router_message(&format!("router already active port={}", config.port));
    }
    Ok(())
}

pub fn log_router_startup_error(message: &str) {
    log_router_message(message);
}

fn start_listener(port: u16) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|error| {
        let message = format!("Local router failed to listen on 127.0.0.1:{port}: {error}");
        log_router_message(&message);
        message
    })?;
    listener
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;

    thread::spawn(move || {
        log_router_message(&format!("router listening 127.0.0.1:{port}"));
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let _ = handle_connection(stream);
            });
        }
    });

    Ok(())
}

fn current_config() -> Result<RouterConfig, String> {
    ACTIVE_ROUTER
        .get()
        .and_then(|state| state.lock().ok())
        .and_then(|guard| guard.clone())
        .ok_or_else(|| "Local router is not configured.".to_string())
}

fn handle_connection(mut stream: TcpStream) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    let request = read_http_request(&mut stream)?;
    let config = current_config()?;

    if request.method == "GET" && request.path == "/health" {
        return write_json_response(&mut stream, 200, router_health(&config));
    }

    if request.method == "GET" && matches!(request.path.as_str(), "/v1/responses" | "/responses") {
        let mut health = router_health(&config);
        health["endpoint"] = json!({
            "method": "POST",
            "path": request.path,
            "description": "Codex Toolkit accepts OpenAI Responses requests here and routes them to a Chat Completions upstream."
        });
        return write_json_response(&mut stream, 200, health);
    }

    if request.method != "POST" {
        return write_json_response(
            &mut stream,
            405,
            json!({"error": {"message": "Only POST is supported by Codex Toolkit local router."}}),
        );
    }

    if request.path == "/v1/responses" || request.path == "/responses" {
        proxy_responses_request(&mut stream, &config, &request.body)
    } else {
        proxy_passthrough_request(&mut stream, &config, &request.path, &request.body)
    }
}

fn router_health(config: &RouterConfig) -> Value {
    json!({
        "ok": true,
        "gateway": "codex-toolkit-local-router",
        "codexWireApi": "responses",
        "upstreamWireApi": config.upstream_wire_api,
        "upstreamBaseUrl": config.upstream_base_url,
        "upstreamModel": config.upstream_model,
        "routes": ["POST /v1/responses", "POST /responses"]
    })
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 1024];
    let header_end;

    loop {
        let read = stream.read(&mut temp).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("HTTP request ended before headers.".to_string());
        }
        buffer.extend_from_slice(&temp[..read]);
        if let Some(index) = find_header_end(&buffer) {
            header_end = index;
            break;
        }
        if buffer.len() > 64 * 1024 {
            return Err("HTTP headers are too large.".to_string());
        }
    }

    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header_text.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| "HTTP request line is missing.".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_string();
    let path = request_parts.next().unwrap_or_default().to_string();

    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    let body_start = header_end + 4;
    let mut body = buffer.get(body_start..).unwrap_or_default().to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut temp).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&temp[..read]);
    }
    body.truncate(content_length);

    Ok(HttpRequest { method, path, body })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn proxy_responses_request(
    stream: &mut TcpStream,
    config: &RouterConfig,
    body: &[u8],
) -> Result<(), String> {
    let response_request: Value =
        serde_json::from_slice(body).map_err(|error| format!("Invalid JSON body: {error}"))?;
    let stream_enabled = response_request
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let chat_request = responses_to_chat_request(&response_request, config);
    let upstream_url = format!("{}/chat/completions", config.upstream_base_url);
    let upstream_model = chat_request
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    log_router_message(&format!(
        "proxy responses stream={} upstream_model={}",
        stream_enabled, upstream_model
    ));

    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|error| error.to_string())?;
    let upstream = client
        .post(upstream_url)
        .bearer_auth(&config.api_key)
        .json(&chat_request)
        .send()
        .map_err(|error| error.to_string())?;

    if !upstream.status().is_success() {
        let status = upstream.status().as_u16();
        let text = upstream.text().unwrap_or_default();
        return write_json_response(stream, status, upstream_error_response(status, &text));
    }

    if stream_enabled {
        proxy_streaming_chat_response(stream, upstream, &response_request)
    } else {
        let chat_response: Value = upstream.json().map_err(|error| error.to_string())?;
        let response = chat_to_responses_response(&chat_response, &response_request);
        write_json_response(stream, 200, response)
    }
}

fn proxy_passthrough_request(
    stream: &mut TcpStream,
    config: &RouterConfig,
    path: &str,
    body: &[u8],
) -> Result<(), String> {
    let upstream_url = format!(
        "{}{}",
        config.upstream_base_url,
        path.strip_prefix("/v1").unwrap_or(path)
    );
    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|error| error.to_string())?;
    let upstream = client
        .post(upstream_url)
        .bearer_auth(&config.api_key)
        .header("content-type", "application/json")
        .body(body.to_vec())
        .send()
        .map_err(|error| error.to_string())?;
    let status = upstream.status().as_u16();
    let text = upstream.text().unwrap_or_default();
    write_raw_response(stream, status, "application/json", text.as_bytes())
}

fn responses_to_chat_request(response_request: &Value, config: &RouterConfig) -> Value {
    let mut request = Map::new();
    let model = config
        .upstream_model
        .clone()
        .or_else(|| {
            response_request
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "deepseek-chat".to_string());

    request.insert("model".to_string(), json!(model));
    request.insert(
        "messages".to_string(),
        Value::Array(extract_messages(response_request)),
    );

    copy_number(response_request, &mut request, "temperature");
    copy_number(response_request, &mut request, "top_p");
    copy_number(response_request, &mut request, "max_output_tokens");
    if let Some(value) = request.remove("max_output_tokens") {
        request.insert("max_tokens".to_string(), value);
    }

    if response_request
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        request.insert("stream".to_string(), Value::Bool(true));
    }

    if let Some(reasoning) = response_request.get("reasoning").and_then(Value::as_object) {
        if let Some(effort) = reasoning.get("effort").and_then(Value::as_str) {
            request.insert("reasoning_effort".to_string(), json!(effort));
        }
    }

    if let Some(tools) = response_request.get("tools").and_then(Value::as_array) {
        let converted_tools: Vec<Value> = tools
            .iter()
            .filter_map(response_tool_to_chat_tool)
            .collect();
        if !converted_tools.is_empty() {
            request.insert("tools".to_string(), Value::Array(converted_tools));
        }
    }

    Value::Object(request)
}

fn copy_number(source: &Value, target: &mut Map<String, Value>, key: &str) {
    if let Some(value) = source.get(key).filter(|value| value.is_number()) {
        target.insert(key.to_string(), value.clone());
    }
}

fn extract_messages(response_request: &Value) -> Vec<Value> {
    let mut messages = Vec::new();
    if let Some(instructions) = response_request
        .get("instructions")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        messages.push(json!({"role": "system", "content": instructions}));
    }

    match response_request.get("input") {
        Some(Value::String(text)) => messages.push(json!({"role": "user", "content": text})),
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(message) = response_item_to_chat_message(item) {
                    messages.push(message);
                }
            }
        }
        _ => {}
    }

    if messages.is_empty() {
        messages.push(json!({"role": "user", "content": ""}));
    }
    messages
}

fn response_item_to_chat_message(item: &Value) -> Option<Value> {
    match item.get("type").and_then(Value::as_str) {
        Some("function_call_output") => return function_call_output_to_chat_message(item),
        Some("function_call") => return function_call_item_to_chat_message(item),
        _ => {}
    }

    let role = item
        .get("role")
        .and_then(Value::as_str)
        .map(normalize_chat_role)
        .unwrap_or("user");
    let content = item
        .get("content")
        .map(response_content_to_text)
        .filter(|value| !value.is_empty())
        .or_else(|| item.get("text").and_then(Value::as_str).map(str::to_string))?;
    Some(json!({"role": role, "content": content}))
}

fn function_call_output_to_chat_message(item: &Value) -> Option<Value> {
    let call_id = item
        .get("call_id")
        .or_else(|| item.get("callId"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())?;
    let content = item
        .get("output")
        .map(response_content_to_text)
        .filter(|value| !value.is_empty())
        .or_else(|| item.get("content").map(response_content_to_text))
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    Some(json!({
        "role": "tool",
        "tool_call_id": call_id,
        "content": content
    }))
}

fn function_call_item_to_chat_message(item: &Value) -> Option<Value> {
    let call_id = item
        .get("call_id")
        .or_else(|| item.get("callId"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("call_codex_toolkit");
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())?;
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(json!({
        "role": "assistant",
        "content": "",
        "tool_calls": [{
            "id": call_id,
            "type": "function",
            "function": {
                "name": name,
                "arguments": arguments
            }
        }]
    }))
}

fn normalize_chat_role(role: &str) -> &'static str {
    match role {
        "system" | "developer" => "system",
        "assistant" => "assistant",
        "tool" => "tool",
        _ => "user",
    }
}

fn response_content_to_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .or_else(|| item.get("input_text"))
                    .or_else(|| item.get("output_text"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        _ => serde_json::to_string(content).unwrap_or_default(),
    }
}

fn response_tool_to_chat_tool(tool: &Value) -> Option<Value> {
    if tool.get("type").and_then(Value::as_str)? != "function" {
        return None;
    }
    Some(json!({
        "type": "function",
        "function": {
            "name": tool.get("name")?.clone(),
            "description": tool.get("description").cloned().unwrap_or(Value::String(String::new())),
            "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({"type": "object", "properties": {}}))
        }
    }))
}

fn tool_call_response_item(tool: &StreamToolCallState) -> Value {
    json!({
        "id": tool.item_id,
        "type": "function_call",
        "status": "completed",
        "call_id": tool.call_id,
        "name": tool.name,
        "arguments": tool.arguments
    })
}

fn chat_tool_call_to_response_item(tool_call: &Value, index: usize) -> Option<Value> {
    let function = tool_call.get("function")?;
    let call_id = tool_call
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("call_codex_toolkit_{index}"));
    let name = function
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = function
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(json!({
        "id": format!("fc_{}", call_id),
        "type": "function_call",
        "status": "completed",
        "call_id": call_id,
        "name": name,
        "arguments": arguments
    }))
}

fn chat_to_responses_response(chat_response: &Value, original_request: &Value) -> Value {
    let message = chat_response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"));
    let output_text = message
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let explicit_reasoning = message.and_then(message_reasoning_text);
    let (inline_reasoning, clean_output_text) = split_inline_think_text(output_text);
    let reasoning_text = explicit_reasoning.or(inline_reasoning);
    let model = chat_response
        .get("model")
        .or_else(|| original_request.get("model"))
        .cloned()
        .unwrap_or_else(|| json!("deepseek-chat"));
    let usage = chat_response.get("usage").cloned().unwrap_or(Value::Null);
    let mut output = Vec::new();
    if let Some(reasoning) = reasoning_text.filter(|value| !value.is_empty()) {
        output.push(json!({
            "id": "rs_codex_toolkit",
            "type": "reasoning",
            "summary": [{
                "type": "summary_text",
                "text": reasoning
            }]
        }));
    }
    output.push(json!({
        "id": "msg_codex_toolkit",
        "type": "message",
        "status": "completed",
        "role": "assistant",
        "content": [{
            "type": "output_text",
            "text": clean_output_text,
            "annotations": []
        }]
    }));
    if let Some(tool_calls) = message
        .and_then(|message| message.get("tool_calls"))
        .and_then(Value::as_array)
    {
        for (index, tool_call) in tool_calls.iter().enumerate() {
            if let Some(item) = chat_tool_call_to_response_item(tool_call, index) {
                output.push(item);
            }
        }
    }

    json!({
        "id": chat_response.get("id").and_then(Value::as_str).unwrap_or("resp_codex_toolkit"),
        "object": "response",
        "created_at": chat_response.get("created").and_then(Value::as_i64).unwrap_or(0),
        "model": model,
        "status": "completed",
        "output": output,
        "usage": normalize_usage(&usage)
    })
}

fn normalize_usage(usage: &Value) -> Value {
    if !usage.is_object() {
        return Value::Null;
    }
    json!({
        "input_tokens": usage.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
        "output_tokens": usage.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0),
        "total_tokens": usage.get("total_tokens").and_then(Value::as_u64).unwrap_or(0)
    })
}

fn proxy_streaming_chat_response(
    stream: &mut TcpStream,
    mut upstream: reqwest::blocking::Response,
    original_request: &Value,
) -> Result<(), String> {
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n",
        )
        .map_err(|error| error.to_string())?;

    let model = original_request
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("codex-toolkit-router")
        .to_string();
    let mut response_state = ResponseStreamState::new(model);
    let mut inline_think = InlineThinkParser::new();
    write_sse_events(stream, response_state.start_events())?;

    let mut bytes = [0_u8; 8192];
    let mut pending = String::new();
    loop {
        let read = match upstream.read(&mut bytes) {
            Ok(read) => read,
            Err(error) => {
                write_sse_events(
                    stream,
                    response_state.failed_events(&format!("Upstream stream read failed: {error}")),
                )?;
                stream
                    .write_all(b"data: [DONE]\n\n")
                    .map_err(|error| error.to_string())?;
                return Ok(());
            }
        };
        if read == 0 {
            break;
        }
        pending.push_str(&String::from_utf8_lossy(&bytes[..read]));
        while let Some(index) = pending.find('\n') {
            let line = pending[..index].trim().to_string();
            pending = pending[index + 1..].to_string();
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data == "[DONE]" {
                    write_sse_events(stream, response_state.done_events())?;
                    stream
                        .write_all(b"data: [DONE]\n\n")
                        .map_err(|error| error.to_string())?;
                    return Ok(());
                }
                if let Ok(chunk) = serde_json::from_str::<Value>(data) {
                    response_state.set_usage(chat_chunk_usage(&chunk));
                    response_state.set_finish_reason(chat_chunk_finish_reason(&chunk));
                    if let Some(reasoning) = chat_chunk_reasoning_delta(&chunk) {
                        write_sse_events(
                            stream,
                            response_state.reasoning_delta_events(&reasoning),
                        )?;
                    }
                    if let Some(delta) = chat_chunk_delta(&chunk) {
                        for inline_delta in inline_think.push(&delta) {
                            match inline_delta.kind {
                                InlineDeltaKind::Reasoning => write_sse_events(
                                    stream,
                                    response_state.reasoning_delta_events(&inline_delta.text),
                                )?,
                                InlineDeltaKind::Text => {
                                    write_sse_events(
                                        stream,
                                        response_state.delta_events(&inline_delta.text),
                                    )?;
                                }
                            }
                        }
                    }
                    for tool_call in chat_chunk_tool_calls(&chunk) {
                        write_sse_events(stream, response_state.tool_call_added_events(tool_call))?;
                        write_sse_events(stream, response_state.tool_call_delta_events(tool_call))?;
                    }
                    if chat_chunk_is_finished(&chunk) {
                        write_inline_think_events(stream, &mut response_state, &mut inline_think)?;
                        write_sse_events(stream, response_state.done_events())?;
                        stream
                            .write_all(b"data: [DONE]\n\n")
                            .map_err(|error| error.to_string())?;
                        return Ok(());
                    }
                }
            }
        }
    }

    write_inline_think_events(stream, &mut response_state, &mut inline_think)?;
    write_sse_events(stream, response_state.done_events())?;
    stream
        .write_all(b"data: [DONE]\n\n")
        .map_err(|error| error.to_string())
}

fn write_inline_think_events(
    stream: &mut TcpStream,
    response_state: &mut ResponseStreamState,
    inline_think: &mut InlineThinkParser,
) -> Result<(), String> {
    for inline_delta in inline_think.finish() {
        match inline_delta.kind {
            InlineDeltaKind::Reasoning => write_sse_events(
                stream,
                response_state.reasoning_delta_events(&inline_delta.text),
            )?,
            InlineDeltaKind::Text => {
                write_sse_events(stream, response_state.delta_events(&inline_delta.text))?;
            }
        }
    }
    Ok(())
}

fn chat_chunk_delta(chunk: &Value) -> Option<String> {
    chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn chat_chunk_reasoning_delta(chunk: &Value) -> Option<String> {
    let delta = chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))?;
    delta
        .get("reasoning_content")
        .or_else(|| delta.get("reasoning"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn chat_chunk_tool_calls(chunk: &Value) -> Vec<&Value> {
    chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("tool_calls"))
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn chat_chunk_finish_reason(chunk: &Value) -> Option<String> {
    chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("finish_reason"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn chat_chunk_usage(chunk: &Value) -> Option<Value> {
    chunk
        .get("usage")
        .filter(|usage| usage.is_object())
        .cloned()
}

fn message_reasoning_text(message: &Value) -> Option<String> {
    message
        .get("reasoning_content")
        .or_else(|| message.get("reasoning"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn split_inline_think_text(text: &str) -> (Option<String>, String) {
    let mut parser = InlineThinkParser::new();
    let mut reasoning = String::new();
    let mut output = String::new();
    for delta in parser.push(text).into_iter().chain(parser.finish()) {
        match delta.kind {
            InlineDeltaKind::Reasoning => reasoning.push_str(&delta.text),
            InlineDeltaKind::Text => output.push_str(&delta.text),
        }
    }
    (Some(reasoning).filter(|value| !value.is_empty()), output)
}

fn partial_tag_suffix_len(text: &str, tag: &str) -> usize {
    let max = text.len().min(tag.len().saturating_sub(1));
    (1..=max)
        .rev()
        .find(|length| {
            let start = text.len() - length;
            text.is_char_boundary(start) && tag.starts_with(&text[start..])
        })
        .unwrap_or(0)
}

fn chat_chunk_is_finished(chunk: &Value) -> bool {
    chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("finish_reason"))
        .is_some_and(|value| !value.is_null())
}

fn upstream_error_response(status: u16, body: &str) -> Value {
    let parsed = serde_json::from_str::<Value>(body).ok();
    let upstream_error = parsed.as_ref().and_then(|value| value.get("error"));
    let message = upstream_error
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| {
            parsed
                .as_ref()
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| {
            if body.trim().is_empty() {
                "Upstream request failed."
            } else {
                body
            }
        });
    let code = upstream_error
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("upstream_error");
    json!({
        "error": {
            "message": message,
            "type": response_error_type(status),
            "code": code,
            "status": status
        }
    })
}

fn response_error_type(status: u16) -> &'static str {
    match status {
        401 | 403 => "authentication_error",
        404 => "not_found_error",
        429 => "rate_limit_error",
        400..=499 => "invalid_request_error",
        _ => "server_error",
    }
}

fn write_sse_events(
    stream: &mut TcpStream,
    events: Vec<(&'static str, Value)>,
) -> Result<(), String> {
    for (event, payload) in events {
        write_sse_event(stream, event, payload)?;
    }
    Ok(())
}

fn write_sse_event(stream: &mut TcpStream, event: &str, data: Value) -> Result<(), String> {
    let payload = format!("event: {event}\ndata: {data}\n\n");
    stream
        .write_all(payload.as_bytes())
        .map_err(|error| error.to_string())?;
    let _ = stream.flush();
    Ok(())
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn log_router_message(message: &str) {
    let Some(path) = router_log_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let line = format!("{} {message}\n", unix_timestamp());
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(line.as_bytes()));
}

fn router_log_path() -> Option<PathBuf> {
    env::var("USERPROFILE")
        .or_else(|_| env::var("HOME"))
        .ok()
        .map(PathBuf::from)
        .map(|home| home.join(".codexviewer").join("local-router.log"))
}

fn write_json_response(stream: &mut TcpStream, status: u16, value: Value) -> Result<(), String> {
    let body = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
    write_raw_response(stream, status, "application/json", &body)
}

fn write_raw_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Upstream",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .and_then(|_| stream.write_all(body))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_response_input_to_chat_messages() {
        let request = json!({
            "model": "gpt-5",
            "instructions": "Be concise.",
            "input": "hello",
            "max_output_tokens": 128,
            "reasoning": {"effort": "high"}
        });
        let config = RouterConfig {
            port: 15721,
            upstream_base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            upstream_model: Some("deepseek-chat".to_string()),
            upstream_wire_api: "chat_completions".to_string(),
        };

        let chat = responses_to_chat_request(&request, &config);

        assert_eq!(chat["model"], "deepseek-chat");
        assert_eq!(chat["max_tokens"], 128);
        assert_eq!(chat["reasoning_effort"], "high");
        assert_eq!(chat["messages"][0]["role"], "system");
        assert_eq!(chat["messages"][1]["content"], "hello");
    }

    #[test]
    fn converts_function_call_output_to_chat_tool_message() {
        let request = json!({
            "model": "gpt-5",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "D:\\codexviewer"
            }]
        });
        let config = RouterConfig {
            port: 15721,
            upstream_base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            upstream_model: Some("deepseek-chat".to_string()),
            upstream_wire_api: "chat_completions".to_string(),
        };

        let chat = responses_to_chat_request(&request, &config);

        assert_eq!(chat["messages"][0]["role"], "tool");
        assert_eq!(chat["messages"][0]["tool_call_id"], "call_1");
        assert_eq!(chat["messages"][0]["content"], "D:\\codexviewer");
    }

    #[test]
    fn converts_function_call_context_to_chat_messages() {
        let request = json!({
            "model": "gpt-5",
            "input": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "shell_command",
                    "arguments": "{\"command\":\"pwd\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": {"stdout": "D:\\codexviewer"}
                }
            ]
        });
        let config = RouterConfig {
            port: 15721,
            upstream_base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            upstream_model: Some("deepseek-chat".to_string()),
            upstream_wire_api: "chat_completions".to_string(),
        };

        let chat = responses_to_chat_request(&request, &config);

        assert_eq!(chat["messages"][0]["role"], "assistant");
        assert_eq!(chat["messages"][0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(chat["messages"][1]["role"], "tool");
        assert_eq!(chat["messages"][1]["tool_call_id"], "call_1");
        assert_eq!(
            chat["messages"][1]["content"],
            "{\"stdout\":\"D:\\\\codexviewer\"}"
        );
    }

    #[test]
    fn converts_chat_response_to_response_shape() {
        let chat = json!({
            "id": "chatcmpl-1",
            "created": 123,
            "model": "deepseek-chat",
            "choices": [{"message": {"role": "assistant", "content": "done"}}],
            "usage": {"prompt_tokens": 2, "completion_tokens": 3, "total_tokens": 5}
        });

        let response = chat_to_responses_response(&chat, &json!({}));

        assert_eq!(response["object"], "response");
        assert_eq!(response["output"][0]["content"][0]["text"], "done");
        assert_eq!(response["usage"]["total_tokens"], 5);
    }

    #[test]
    fn converts_non_stream_reasoning_to_response_output() {
        let chat = json!({
            "id": "chatcmpl-1",
            "created": 123,
            "model": "deepseek-chat",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "check facts",
                    "content": "done"
                }
            }]
        });

        let response = chat_to_responses_response(&chat, &json!({}));

        assert_eq!(response["output"][0]["type"], "reasoning");
        assert_eq!(response["output"][0]["summary"][0]["text"], "check facts");
        assert_eq!(response["output"][1]["content"][0]["text"], "done");
    }

    #[test]
    fn converts_non_stream_tool_calls_to_response_items() {
        let chat = json!({
            "id": "chatcmpl-1",
            "created": 123,
            "model": "deepseek-chat",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "shell_command",
                            "arguments": "{\"command\":\"pwd\"}"
                        }
                    }]
                }
            }]
        });

        let response = chat_to_responses_response(&chat, &json!({}));

        assert_eq!(response["output"][1]["type"], "function_call");
        assert_eq!(response["output"][1]["call_id"], "call_1");
        assert_eq!(response["output"][1]["name"], "shell_command");
        assert_eq!(response["output"][1]["arguments"], "{\"command\":\"pwd\"}");
    }

    #[test]
    fn response_stream_events_have_complete_lifecycle() {
        let mut state = ResponseStreamState::new("gpt-5.5".to_string());
        let mut events = state.start_events();
        events.extend(state.delta_events("po"));
        events.extend(state.delta_events("ng"));
        events.extend(state.done_events());

        let names: Vec<&str> = events.iter().map(|(name, _)| *name).collect();

        assert_eq!(
            names,
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed"
            ]
        );
        let completed = events.last().unwrap().1.clone();
        assert_eq!(completed["type"], "response.completed");
        assert_eq!(completed["response"]["status"], "completed");
        assert_eq!(
            completed["response"]["output"][0]["content"][0]["text"],
            "pong"
        );
    }

    #[test]
    fn response_stream_events_include_reasoning_lifecycle() {
        let mut state = ResponseStreamState::new("gpt-5.5".to_string());
        let mut events = state.start_events();
        events.extend(state.reasoning_delta_events("Need context."));
        events.extend(state.delta_events("pong"));
        events.extend(state.done_events());

        let names: Vec<&str> = events.iter().map(|(name, _)| *name).collect();

        assert!(names.contains(&"response.reasoning_summary_part.added"));
        assert!(names.contains(&"response.reasoning_summary_text.delta"));
        assert!(names.contains(&"response.reasoning_summary_text.done"));
        assert!(names.contains(&"response.reasoning_summary_part.done"));
        let completed = events.last().unwrap().1.clone();
        assert_eq!(
            completed["response"]["output"][0]["summary"][0]["text"],
            "Need context."
        );
        assert_eq!(
            completed["response"]["output"][1]["content"][0]["text"],
            "pong"
        );
    }

    #[test]
    fn response_stream_events_include_tool_call_lifecycle() {
        let mut state = ResponseStreamState::new("gpt-5.5".to_string());
        let first = json!({
            "index": 0,
            "id": "call_1",
            "type": "function",
            "function": {"name": "shell_command", "arguments": "{\"command\":"}
        });
        let second = json!({
            "index": 0,
            "function": {"arguments": "\"pwd\"}"}
        });
        let mut events = state.start_events();
        events.extend(state.tool_call_added_events(&first));
        events.extend(state.tool_call_delta_events(&first));
        events.extend(state.tool_call_added_events(&second));
        events.extend(state.tool_call_delta_events(&second));
        events.extend(state.done_events());

        let names: Vec<&str> = events.iter().map(|(name, _)| *name).collect();
        assert!(names.contains(&"response.output_item.added"));
        assert!(names.contains(&"response.function_call_arguments.delta"));
        assert!(names.contains(&"response.output_item.done"));
        let completed = events.last().unwrap().1.clone();
        assert_eq!(completed["response"]["output"][0]["type"], "function_call");
        assert_eq!(
            completed["response"]["output"][0]["arguments"],
            "{\"command\":\"pwd\"}"
        );
    }

    #[test]
    fn response_stream_marks_length_finish_as_incomplete() {
        let mut state = ResponseStreamState::new("gpt-5.5".to_string());
        state.set_finish_reason(Some("length".to_string()));
        let events = state.done_events();

        let completed = events.last().unwrap().1.clone();
        assert_eq!(completed["response"]["status"], "incomplete");
        assert_eq!(
            completed["response"]["incomplete_details"]["reason"],
            "max_output_tokens"
        );
    }

    #[test]
    fn response_stream_failed_event_uses_responses_error_shape() {
        let mut state = ResponseStreamState::new("gpt-5.5".to_string());
        let events = state.failed_events("network closed");

        assert_eq!(events[0].0, "response.failed");
        assert_eq!(events[0].1["response"]["status"], "failed");
        assert_eq!(events[0].1["error"]["code"], "upstream_stream_error");
    }

    #[test]
    fn extracts_reasoning_delta_from_chat_chunk() {
        let chunk = json!({
            "choices": [{
                "delta": {"reasoning_content": "thinking"}
            }]
        });

        assert_eq!(
            chat_chunk_reasoning_delta(&chunk),
            Some("thinking".to_string())
        );
    }

    #[test]
    fn extracts_tool_calls_from_chat_chunk() {
        let chunk = json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "function": {"name": "shell_command"}
                    }]
                }
            }]
        });

        let tool_calls = chat_chunk_tool_calls(&chunk);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "call_1");
    }

    #[test]
    fn extracts_finish_reason_and_usage_from_chat_chunk() {
        let chunk = json!({
            "choices": [{"finish_reason": "length"}],
            "usage": {"prompt_tokens": 3, "completion_tokens": 4, "total_tokens": 7}
        });

        assert_eq!(chat_chunk_finish_reason(&chunk), Some("length".to_string()));
        assert_eq!(
            normalize_usage(&chat_chunk_usage(&chunk).unwrap())["total_tokens"],
            7
        );
    }

    #[test]
    fn upstream_errors_are_normalized_for_responses_clients() {
        let error = upstream_error_response(
            429,
            r#"{"error":{"message":"slow down","code":"rate_limit"}}"#,
        );

        assert_eq!(error["error"]["message"], "slow down");
        assert_eq!(error["error"]["type"], "rate_limit_error");
        assert_eq!(error["error"]["code"], "rate_limit");
    }

    #[test]
    fn router_health_describes_codex_and_upstream_wire_apis() {
        let config = RouterConfig {
            port: 15721,
            upstream_base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            upstream_model: Some("deepseek-chat".to_string()),
            upstream_wire_api: "chat_completions".to_string(),
        };

        let health = router_health(&config);

        assert_eq!(health["codexWireApi"], "responses");
        assert_eq!(health["upstreamWireApi"], "chat_completions");
    }

    #[test]
    fn splits_inline_think_text() {
        let (reasoning, text) = split_inline_think_text("<think>work</think>answer");

        assert_eq!(reasoning, Some("work".to_string()));
        assert_eq!(text, "answer");
    }

    #[test]
    fn parses_split_inline_think_stream() {
        let mut parser = InlineThinkParser::new();
        let mut deltas = Vec::new();
        deltas.extend(parser.push("<thi"));
        deltas.extend(parser.push("nk>work</thi"));
        deltas.extend(parser.push("nk>answer"));
        deltas.extend(parser.finish());

        assert_eq!(
            deltas,
            vec![
                InlineDelta {
                    kind: InlineDeltaKind::Reasoning,
                    text: "work".to_string()
                },
                InlineDelta {
                    kind: InlineDeltaKind::Text,
                    text: "answer".to_string()
                }
            ]
        );
    }

    #[test]
    fn keeps_partial_open_tag_as_text_when_stream_finishes() {
        let mut parser = InlineThinkParser::new();
        let mut deltas = parser.push("hello <thi");
        deltas.extend(parser.finish());

        let text = deltas
            .iter()
            .filter(|delta| delta.kind == InlineDeltaKind::Text)
            .map(|delta| delta.text.as_str())
            .collect::<String>();
        assert_eq!(text, "hello <thi");
    }

    #[test]
    fn detects_finished_chat_stream_chunk() {
        let chunk = json!({
            "choices": [{
                "delta": {},
                "finish_reason": "stop"
            }]
        });

        assert!(chat_chunk_is_finished(&chunk));
    }
}
