//! `run --protocol acp-v1` must hand the process over to the agent.
//!
//! CadencR stops a provider by killing the pid it spawned. If the connector
//! spawned `vibe-acp` as a child instead of `exec`-ing it, that kill would
//! leave the agent running as an orphan, still holding its ACP session and its
//! turn. The pid the agent reports is therefore the assertion: after `exec`
//! the agent *is* the process CadencR spawned.

#![cfg(unix)]

use std::process::{Command, Stdio};

#[test]
fn run_replaces_the_connector_process_with_the_agent() {
    let provider = env!("CARGO_BIN_EXE_cadencr-plugin-provider-vibe");
    let mut child = Command::new(provider)
        .args(["run", "--protocol", "acp-v1", "-c", "echo agent_pid=$$"])
        .env("CADENCR_VIBE_ACP_PATH", "/bin/sh")
        .stdout(Stdio::piped())
        .spawn()
        .expect("the connector should start");
    let connector_pid = child.id();
    let output = child
        .wait_with_output()
        .expect("the connector should exit with the agent");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let agent_pid: u32 = stdout
        .trim()
        .strip_prefix("agent_pid=")
        .expect("the agent should report its pid")
        .parse()
        .expect("the reported pid should be numeric");

    assert_eq!(
        agent_pid, connector_pid,
        "the agent must inherit the connector's pid, otherwise CadencR's kill leaves it orphaned",
    );
}
