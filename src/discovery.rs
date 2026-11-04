//! Pre-session model discovery against the live `vibe-acp` binary.
//!
//! ACP exposes session configuration only after `session/new`, and Cadencr
//! requires a verified model selector before a session exists. Discovery
//! therefore opens a disposable ACP session in the requested workspace,
//! reads the `configOptions` selectors from the `session/new` response,
//! closes the session when the agent supports it, and returns them verbatim,
//! except for the `mode` selector whose choices are annotated with the host
//! permission-mode id (`_meta.cadencr.permissionMode`) so Cadencr can drive
//! Vibe's session mode from its own mode chip. Because the same binary serves
//! `run`, the live session will advertise the same selector ids and values.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::SessionConfigOption;
use serde_json::{json, Value};

const INITIALIZE_ID: &str = "cadencr-vibe-init";
const SESSION_NEW_ID: &str = "cadencr-vibe-session";
const SESSION_CLOSE_ID: &str = "cadencr-vibe-close";

/// Total discovery budget. The host terminates `models` after 10 seconds,
/// so stay below it to return an actionable error instead of being killed.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(9);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);

const AUTH_HINT: &str = "Mistral Vibe credentials are missing or invalid. \
     Run `vibe --setup` (or export MISTRAL_API_KEY), then retry.";

/// Discover the model and mode selectors for the workspace at `cwd`.
///
/// The model selector is returned verbatim. The `mode` selector is returned
/// with each choice annotated with `_meta.cadencr.permissionMode` so Cadencr
/// maps its permission-mode chip onto Vibe's session modes (see CadencR's
/// `docs/PROVIDER_SPEC/PROVIDER_PACKAGE.md`, "Mode and access-mode selectors").
pub fn discover_model_options(cwd: &Path) -> Result<Vec<SessionConfigOption>, String> {
    let mut session = DiscoverySession::spawn(cwd)?;
    let discovered = session.discover();
    session.close();
    discovered.map(|options| discovery_options(&options))
}

struct DiscoverySession {
    _child: ReapedChild,
    stdin: std::process::ChildStdin,
    lines: mpsc::Receiver<String>,
    deadline: Instant,
    session_id: Option<String>,
}

/// Own the process immediately after spawn, including before ACP initialization.
struct ReapedChild(Child);

impl Drop for ReapedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl DiscoverySession {
    fn spawn(cwd: &Path) -> Result<Self, String> {
        let binary =
            std::env::var_os(crate::runner::VIBE_ACP_PATH_ENV).unwrap_or_else(|| "vibe-acp".into());
        Self::spawn_with_command(Command::new(binary), cwd)
    }

    fn spawn_with_command(mut command: Command, cwd: &Path) -> Result<Self, String> {
        let deadline = Instant::now() + DISCOVERY_TIMEOUT;
        let binary = command.get_program().to_string_lossy().into_owned();
        let mut child = ReapedChild(
            command
                .current_dir(cwd)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|error| {
                    format!(
                        "failed to start `{}`: {error}. Install the Mistral Vibe CLI \
                     (https://mistral.ai/vibe) or set {}",
                        binary,
                        crate::runner::VIBE_ACP_PATH_ENV,
                    )
                })?,
        );
        let stdin = child
            .0
            .stdin
            .take()
            .ok_or_else(|| "`vibe-acp` stdin unavailable".to_string())?;
        let stdout = child
            .0
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
            _child: child,
            stdin,
            lines: rx,
            deadline,
            session_id: None,
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
        // All ACP responses share this deadline; preserve time for cleanup.
        let deadline = self.deadline - CLOSE_TIMEOUT;
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| "`vibe-acp` discovery timed out".to_string())?;
            let line = self
                .lines
                .recv_timeout(remaining)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => "`vibe-acp` discovery timed out".to_string(),
                    mpsc::RecvTimeoutError::Disconnected => {
                        "`vibe-acp` closed stdout before responding".to_string()
                    }
                })?;
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

    /// Read the `session/new` response and return the raw config options.
    fn discover(&mut self) -> Result<Vec<Value>, String> {
        let response = self.wait_response(SESSION_NEW_ID)?;
        self.session_id = response
            .pointer("/result/sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let options = response
            .pointer("/result/configOptions")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                "`vibe-acp` returned no configOptions; cannot discover models".to_string()
            })?;
        let options = options.clone();
        if model_select_option(&options).is_none() {
            return Err("`vibe-acp` exposed no model selector at session creation".to_string());
        }
        Ok(options)
    }

    /// Close a known session within the remaining global budget.
    /// The process guard always kills and reaps the child when we return.
    fn close(&mut self) {
        let Some(session_id) = self.session_id.take() else {
            return;
        };
        let deadline = self.deadline.min(Instant::now() + CLOSE_TIMEOUT);
        if Instant::now() >= deadline
            || self
                .send(json!({
                    "jsonrpc": "2.0",
                    "id": SESSION_CLOSE_ID,
                    "method": "session/close",
                    "params": { "sessionId": session_id },
                }))
                .is_err()
        {
            return;
        }
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
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
    }
}

/// Build the `models` output: the model selector verbatim, plus the `mode`
/// selector with host permission-mode annotations on its choices.
fn discovery_options(options: &[Value]) -> Vec<SessionConfigOption> {
    let mut discovered = Vec::new();
    if let Some(model) = model_select_option(options) {
        discovered.push(model);
    }
    if let Some(mode) = mode_select_option(options) {
        discovered.push(mode);
    }
    discovered
}

/// Host permission-mode wire id for a Vibe session mode value. Values without
/// a mapping are left unannotated; the host ignores them.
fn host_permission_mode(value: &str) -> Option<&'static str> {
    match value {
        "ask" => Some("ask"),
        "accept-edits" => Some("acceptEdits"),
        "smart-approve" => Some("auto"),
        "auto-approve" => Some("bypassPermissions"),
        _ => None,
    }
}

/// Find the `select` option whose category is `mode`, annotate each choice
/// with its host permission-mode id, and validate it through the ACP v1 types.
fn mode_select_option(options: &[Value]) -> Option<SessionConfigOption> {
    let candidate = options.iter().find(|option| {
        option.get("category").and_then(Value::as_str) == Some("mode")
            && option.get("type").and_then(Value::as_str) == Some("select")
    })?;
    let mut candidate = candidate.clone();
    annotate_mode_choices(&mut candidate);
    let option: SessionConfigOption = serde_json::from_value(candidate).ok()?;
    let has_choices = match &option.kind {
        agent_client_protocol::schema::v1::SessionConfigKind::Select(select) => {
            select_offers_choices(&select.options)
        }
        _ => false,
    };
    has_choices.then_some(option)
}

/// Write `_meta.cadencr.permissionMode` on every mapped choice of a mode
/// selector, grouped or ungrouped.
fn annotate_mode_choices(option: &mut Value) {
    fn annotate_array(choices: &mut [Value]) {
        for choice in choices {
            if let Some(object) = choice.as_object_mut() {
                let host_id = object
                    .get("value")
                    .and_then(Value::as_str)
                    .and_then(host_permission_mode);
                if let Some(host_id) = host_id {
                    let meta = object
                        .entry("_meta")
                        .or_insert_with(|| Value::Object(Default::default()));
                    let cadencr = meta
                        .as_object_mut()
                        .expect("freshly inserted _meta is an object")
                        .entry("cadencr")
                        .or_insert_with(|| Value::Object(Default::default()));
                    cadencr
                        .as_object_mut()
                        .expect("freshly inserted cadencr is an object")
                        .insert(
                            "permissionMode".to_string(),
                            Value::String(host_id.to_string()),
                        );
                }
                if let Some(nested) = object.get_mut("options").and_then(Value::as_array_mut) {
                    annotate_array(nested);
                }
            }
        }
    }
    if let Some(choices) = option.get_mut("options").and_then(Value::as_array_mut) {
        annotate_array(choices);
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
    use super::{
        discovery_options, host_permission_mode, mode_select_option, model_select_option,
        rpc_error_message, AUTH_HINT,
    };
    use serde_json::{json, Value};

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

    fn vibe_mode_option() -> serde_json::Value {
        json!({
            "id": "mode",
            "name": "Session Mode",
            "category": "mode",
            "type": "select",
            "currentValue": "accept-edits",
            "options": [
                { "value": "ask", "name": "Ask" },
                { "value": "accept-edits", "name": "Accept Edits" },
                { "value": "smart-approve", "name": "Smart Approve" },
                { "value": "auto-approve", "name": "Auto Approve" },
                { "value": "custom", "name": "Custom" },
            ],
        })
    }

    #[test]
    fn discovery_output_carries_model_and_annotated_mode() {
        let options = vec![vibe_mode_option(), model_option()];
        let discovered = discovery_options(&options);
        assert_eq!(discovered.len(), 2);
        assert_eq!(&*discovered[0].id.0, "model");
        assert_eq!(&*discovered[1].id.0, "mode");
        let serialized = serde_json::to_value(&discovered[1]).unwrap();
        let choices = serialized["options"].as_array().unwrap();
        let mapping = |value: &str| {
            choices
                .iter()
                .find(|choice| choice["value"] == value)
                .and_then(|choice| choice.pointer("/_meta/cadencr/permissionMode"))
                .and_then(Value::as_str)
        };
        assert_eq!(mapping("ask"), Some("ask"));
        assert_eq!(mapping("accept-edits"), Some("acceptEdits"));
        assert_eq!(mapping("smart-approve"), Some("auto"));
        assert_eq!(mapping("auto-approve"), Some("bypassPermissions"));
        assert_eq!(mapping("custom"), None);
    }

    #[test]
    fn mode_selector_is_omitted_when_the_agent_exposes_none() {
        assert_eq!(discovery_options(&[model_option()]).len(), 1);
        assert!(mode_select_option(&[model_option()]).is_none());
    }

    #[test]
    fn mode_selector_must_offer_choices() {
        let mut option = vibe_mode_option();
        option["options"] = json!([]);
        assert!(mode_select_option(&[option]).is_none());
    }

    #[test]
    fn grouped_mode_choices_are_annotated_too() {
        let mut option = vibe_mode_option();
        option["options"] = json!([
            { "group": "builtin", "name": "Builtin", "options": [
                { "value": "auto-approve", "name": "Auto Approve" }
            ] }
        ]);
        let parsed = mode_select_option(&[option]).expect("grouped mode selector expected");
        let serialized = serde_json::to_value(&parsed).unwrap();
        let choice = &serialized["options"][0]["options"][0];
        assert_eq!(
            choice
                .pointer("/_meta/cadencr/permissionMode")
                .and_then(Value::as_str),
            Some("bypassPermissions")
        );
    }

    #[test]
    fn known_vibe_modes_map_to_host_ids() {
        assert_eq!(host_permission_mode("ask"), Some("ask"));
        assert_eq!(host_permission_mode("accept-edits"), Some("acceptEdits"));
        assert_eq!(host_permission_mode("smart-approve"), Some("auto"));
        assert_eq!(
            host_permission_mode("auto-approve"),
            Some("bypassPermissions")
        );
        assert_eq!(host_permission_mode("something-else"), None);
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

    #[cfg(unix)]
    struct FakeAgent {
        directory: std::path::PathBuf,
        scenario: &'static str,
    }

    #[cfg(unix)]
    impl FakeAgent {
        fn new(scenario: &'static str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "cadencr-vibe-test-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
            ));
            std::fs::create_dir(&directory).unwrap();
            Self {
                directory,
                scenario,
            }
        }

        fn spawn(&self) -> Result<super::DiscoverySession, String> {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "discovery::tests::fake_acp_process",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("CADENCR_VIBE_TEST_SCENARIO", self.scenario)
                .env("CADENCR_VIBE_TEST_DIRECTORY", &self.directory);
            super::DiscoverySession::spawn_with_command(command, &self.directory)
        }

        fn pid(&self) -> libc::pid_t {
            std::fs::read_to_string(self.directory.join("pid"))
                .unwrap()
                .parse()
                .unwrap()
        }

        fn assert_reaped(&self) {
            let mut status = 0;
            // A still-running child returns 0; a zombie returns its PID.
            // ECHILD proves the discovery code has already waited for it.
            let result = unsafe { libc::waitpid(self.pid(), &mut status, libc::WNOHANG) };
            let error = std::io::Error::last_os_error();
            assert_eq!(result, -1, "discovery left a running or unreaped child");
            assert_eq!(error.raw_os_error(), Some(libc::ECHILD));
        }

        fn requests(&self) -> String {
            std::fs::read_to_string(self.directory.join("requests")).unwrap()
        }
    }

    #[cfg(unix)]
    impl Drop for FakeAgent {
        fn drop(&mut self) {
            if let Ok(pid) = std::fs::read_to_string(self.directory.join("pid")) {
                if let Ok(pid) = pid.parse::<libc::pid_t>() {
                    let mut status = 0;
                    // Clean up even when a regression assertion fails.
                    if unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) } == 0 {
                        unsafe {
                            libc::kill(pid, libc::SIGKILL);
                            libc::waitpid(pid, &mut status, 0);
                        }
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    #[cfg(unix)]
    #[test]
    fn failed_initialize_terminates_and_reaps_the_child() {
        let agent = FakeAgent::new("initialize-error");
        let error = agent.spawn().err().expect("initialization must fail");
        agent.assert_reaped();
        assert!(error.contains("fixture initialization failed"));
    }

    #[cfg(unix)]
    #[test]
    fn timed_out_initialize_terminates_and_reaps_the_child() {
        let agent = FakeAgent::new("initialize-timeout");
        let started = std::time::Instant::now();
        let error = agent.spawn().err().expect("initialization must time out");
        agent.assert_reaped();
        assert!(error.contains("timed out"), "{error}");
        assert!(started.elapsed() < std::time::Duration::from_millis(9500));
    }

    #[cfg(unix)]
    #[test]
    fn slow_initialize_and_session_new_share_one_deadline() {
        let agent = FakeAgent::new("slow-session");
        let started = std::time::Instant::now();
        let mut session = agent.spawn().expect("seven-second initialization must fit");
        let result = session.discover();
        session.close();
        drop(session);
        agent.assert_reaped();
        assert!(agent.requests().contains("session/new"));
        assert!(
            result.is_err(),
            "seven plus four seconds must exceed the budget"
        );
        assert!(result.err().unwrap().contains("timed out"));
        assert!(started.elapsed() < std::time::Duration::from_millis(9500));
    }

    #[cfg(unix)]
    #[test]
    fn successful_discovery_bounds_cleanup_and_reaps_the_child() {
        let agent = FakeAgent::new("close-timeout");
        let started = std::time::Instant::now();
        let mut session = agent.spawn().unwrap();
        let options = session.discover().expect("model selector expected");
        session.close();
        drop(session);
        agent.assert_reaped();
        assert!(options
            .iter()
            .any(|option| option.get("id").and_then(Value::as_str) == Some("model")));
        assert!(agent.requests().contains("session/close"));
        assert!(started.elapsed() < std::time::Duration::from_millis(1800));
    }

    /// Runs in an isolated copy of this test binary, without a CLI dependency.
    #[cfg(unix)]
    #[test]
    fn fake_acp_process() {
        use std::io::{BufRead, Write};
        use std::time::Duration;
        let Ok(scenario) = std::env::var("CADENCR_VIBE_TEST_SCENARIO") else {
            return;
        };
        let directory =
            std::path::PathBuf::from(std::env::var_os("CADENCR_VIBE_TEST_DIRECTORY").unwrap());
        // End the test harness's prefix before writing JSON-RPC lines.
        println!();
        std::io::stdout().flush().unwrap();
        std::fs::write(directory.join("pid"), std::process::id().to_string()).unwrap();
        let stall = || loop {
            std::thread::sleep(Duration::from_secs(30));
        };
        for line in std::io::stdin().lock().lines() {
            let request: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
            let method = request["method"].as_str().unwrap();
            let mut requests = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(directory.join("requests"))
                .unwrap();
            writeln!(requests, "{method}").unwrap();
            if method == "initialize" && scenario == "initialize-timeout"
                || method == "session/close" && scenario == "close-timeout"
            {
                stall();
            }
            let mut response = json!({"jsonrpc": "2.0", "id": request["id"]});
            match method {
                "initialize" if scenario == "initialize-error" => {
                    response["error"] = json!({
                        "code": -32603,
                        "message": "fixture initialization failed"
                    });
                }
                "initialize" => {
                    if scenario == "slow-session" {
                        std::thread::sleep(Duration::from_secs(7));
                    }
                    response["result"] = json!({"protocolVersion": 1});
                }
                "session/new" => {
                    if scenario == "slow-session" {
                        std::thread::sleep(Duration::from_secs(4));
                    }
                    response["result"] = json!({
                        "sessionId": "fixture-session",
                        "configOptions": [model_option()]
                    });
                }
                "session/close" => response["result"] = serde_json::Value::Null,
                _ => panic!("unexpected request {method}"),
            }
            println!("{response}");
            std::io::stdout().flush().unwrap();
            if scenario == "initialize-error" {
                stall();
            }
        }
    }
}
