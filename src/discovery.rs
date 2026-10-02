//! Pre-session model discovery against the live `vibe-acp` binary.
//!
//! ACP exposes session configuration only after `session/new`, and Cadencr
//! requires a verified model selector before a session exists. Discovery
//! therefore opens a disposable ACP session in the requested workspace,
//! reads the `configOptions` model selector from the `session/new` response,
//! closes the session when the agent supports it, and returns the selector
//! verbatim. Because the same binary serves `run`, the live session will
//! advertise the same selector id and values.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use agent_client_protocol::schema::v1::SessionConfigOption;
use serde_json::{json, Value};

const INITIALIZE_ID: &str = "cadencr-vibe-init";
const SESSION_NEW_ID: &str = "cadencr-vibe-session";
const SESSION_CLOSE_ID: &str = "cadencr-vibe-close";

/// Total discovery budget. The host terminates `models` after 10 seconds,
/// so stay below it to return an actionable error instead of being killed.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(8);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

const AUTH_HINT: &str = "Mistral Vibe credentials are missing or invalid. \
     Run `vibe --setup` (or export MISTRAL_API_KEY), then retry.";

/// Discover the model selector for the workspace at `cwd`.
pub fn discover_model_options(cwd: &Path) -> Result<Vec<SessionConfigOption>, String> {
    let mut session = DiscoverySession::spawn(cwd)?;
    let discovered = session.discover();
    session.close();
    discovered.map(|option| vec![option])
}

struct DiscoverySession {
    child: Child,
    stdin: std::process::ChildStdin,
    lines: mpsc::Receiver<String>,
}

impl DiscoverySession {
    fn spawn(cwd: &Path) -> Result<Self, String> {
        let binary =
            std::env::var_os(crate::runner::VIBE_ACP_PATH_ENV).unwrap_or_else(|| "vibe-acp".into());
        let mut child = Command::new(&binary)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                format!(
                    "failed to start `{}`: {error}. Install the Mistral Vibe CLI \
                     (https://mistral.ai/vibe) or set {}",
                    binary.to_string_lossy(),
                    crate::runner::VIBE_ACP_PATH_ENV,
                )
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "`vibe-acp` stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "`vibe-acp` stdout unavailable".to_string())?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });
        let mut session = Self {
            child,
            stdin,
            lines: rx,
        };
        session.initialize(cwd)?;
        Ok(session)
    }

    fn send(&mut self, request: Value) -> Result<(), String> {
        writeln!(self.stdin, "{request}")
            .map_err(|error| format!("failed to write to `vibe-acp`: {error}"))
    }

    /// Read stdout lines until a response with the given id arrives.
    /// Notifications and unrelated responses are skipped.
    fn wait_response(&mut self, id: &str) -> Result<Value, String> {
        let deadline = std::time::Instant::now() + DISCOVERY_TIMEOUT;
        loop {
            let remaining = deadline
                .checked_duration_since(std::time::Instant::now())
                .ok_or_else(|| "`vibe-acp` discovery timed out".to_string())?;
            let line = self
                .lines
                .recv_timeout(remaining)
                .map_err(|_| "`vibe-acp` closed stdout before responding".to_string())?;
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if value.get("id").and_then(Value::as_str) == Some(id) {
                if let Some(error) = value.get("error") {
                    return Err(rpc_error_message(id, error));
                }
                return Ok(value);
            }
        }
    }

    fn initialize(&mut self, cwd: &Path) -> Result<(), String> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": INITIALIZE_ID,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": {},
                "clientInfo": {
                    "name": "cadencr-plugin-provider-vibe",
                    "title": "Cadencr Vibe connector",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            },
        });
        self.send(request)?;
        self.wait_response(INITIALIZE_ID)?;
        let cwd = cwd.to_string_lossy().into_owned();
        let request = json!({
            "jsonrpc": "2.0",
            "id": SESSION_NEW_ID,
            "method": "session/new",
            // vibe-acp treats mcpServers as a required field even though
            // ACP v1 marks it optional; send an explicit empty list.
            "params": { "cwd": cwd, "mcpServers": [] },
        });
        self.send(request)?;
        Ok(())
    }

    /// Read the `session/new` response and extract the model selector.
    fn discover(&mut self) -> Result<SessionConfigOption, String> {
        let response = self.wait_response(SESSION_NEW_ID)?;
        let session_id = response
            .pointer("/result/sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let options = response
            .pointer("/result/configOptions")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                "`vibe-acp` returned no configOptions; cannot discover models".to_string()
            })?;
        let option = model_select_option(options).ok_or_else(|| {
            "`vibe-acp` exposed no model selector at session creation".to_string()
        })?;
        if let Some(session_id) = session_id {
            let request = json!({
                "jsonrpc": "2.0",
                "id": SESSION_CLOSE_ID,
                "method": "session/close",
                "params": { "sessionId": session_id },
            });
            let _ = self.send(request);
        }
        Ok(option)
    }

    /// Best-effort cleanup: wait briefly for the close response, then kill.
    fn close(&mut self) {
        let deadline = std::time::Instant::now() + CLOSE_TIMEOUT;
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match self.lines.recv_timeout(remaining) {
                Ok(line) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&line) {
                        if value.get("id").and_then(Value::as_str) == Some(SESSION_CLOSE_ID) {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Find the `select` option whose category is `model`, falling back to the
/// conventional id `model`, and validate it through the ACP v1 types.
fn model_select_option(options: &[Value]) -> Option<SessionConfigOption> {
    let candidate = options
        .iter()
        .find(|option| {
            option.get("category").and_then(Value::as_str) == Some("model")
                && option.get("type").and_then(Value::as_str) == Some("select")
        })
        .or_else(|| {
            options
                .iter()
                .find(|option| option.get("id").and_then(Value::as_str) == Some("model"))
        })?;
    let option: SessionConfigOption = serde_json::from_value(candidate.clone()).ok()?;
    let has_choices = match &option.kind {
        agent_client_protocol::schema::v1::SessionConfigKind::Select(select) => {
            select_offers_choices(&select.options)
        }
        _ => false,
    };
    has_choices.then_some(option)
}

/// True when a select payload offers at least one value, grouped or not.
fn select_offers_choices(
    options: &agent_client_protocol::schema::v1::SessionConfigSelectOptions,
) -> bool {
    use agent_client_protocol::schema::v1::SessionConfigSelectOptions;
    match options {
        SessionConfigSelectOptions::Ungrouped(options) => !options.is_empty(),
        SessionConfigSelectOptions::Grouped(groups) => {
            groups.iter().any(|group| !group.options.is_empty())
        }
        _ => false,
    }
}

/// Turn a JSON-RPC error object into an actionable diagnostic.
fn rpc_error_message(request: &str, error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown error");
    let text = format!("`vibe-acp` rejected {request}: {message}");
    let lower = message.to_lowercase();
    if lower.contains("auth") || lower.contains("credential") || lower.contains("api key") {
        format!("{text}. {AUTH_HINT}")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::{model_select_option, rpc_error_message, AUTH_HINT};
    use serde_json::json;

    fn model_option() -> serde_json::Value {
        json!({
            "id": "model",
            "name": "Model",
            "description": "Active model",
            "category": "model",
            "type": "select",
            "currentValue": "mistral-medium-3.5",
            "options": [
                { "value": "mistral-medium-3.5", "name": "Mistral Medium 3.5" },
                { "value": "local", "name": "Devstral (local)" },
            ],
        })
    }

    #[test]
    fn extracts_the_model_select_option() {
        let options = vec![
            json!({
                "id": "mode",
                "name": "Mode",
                "category": "mode",
                "type": "select",
                "currentValue": "accept-edits",
                "options": [{ "value": "accept-edits", "name": "Accept edits" }],
            }),
            model_option(),
        ];
        let option = model_select_option(&options).expect("model selector expected");
        assert_eq!(&*option.id.0, "model");
    }

    #[test]
    fn rejects_empty_model_choices() {
        let mut option = model_option();
        option["options"] = json!([]);
        assert!(model_select_option(&[option]).is_none());
    }

    #[test]
    fn falls_back_to_the_model_id_without_category() {
        let mut option = model_option();
        option.as_object_mut().unwrap().remove("category");
        assert!(model_select_option(&[option]).is_some());
    }

    #[test]
    fn auth_errors_carry_the_setup_hint() {
        let error = json!({ "code": -32000, "message": "ACP authentication required" });
        let message = rpc_error_message("session/new", &error);
        assert!(message.contains(AUTH_HINT));
    }

    #[test]
    fn other_errors_keep_the_raw_message() {
        let error = json!({ "code": -32600, "message": "invalid request" });
        let message = rpc_error_message("initialize", &error);
        assert!(!message.contains(AUTH_HINT));
        assert!(message.contains("invalid request"));
    }
}
