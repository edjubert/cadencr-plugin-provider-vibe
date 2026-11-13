//! `run --protocol acp-v1`: hand stdio to the native `vibe-acp` binary.
//!
//! Vibe speaks ACP v1 natively, so the connector adds no translation layer.
//! The binary is resolved from `CADENCR_VIBE_ACP_PATH`, defaulting to
//! `vibe-acp` on `PATH`. Environment is inherited unchanged so the native
//! CLI keeps its own credentials (`~/.vibe/.env` or `MISTRAL_API_KEY`).
//!
//! On Unix the connector `exec`s the binary rather than spawning it, so the
//! process CadencR owns *is* the agent. CadencR stops a provider by killing
//! the pid it spawned, and an imported provider runs under the `Inherit`
//! process-tree policy: a spawned `vibe-acp` would survive that kill as an
//! orphan, still holding its ACP session and its turn. `exec` keeps the pid
//! and the inherited stdio, so the kill lands on the agent itself.

use std::ffi::{OsStr, OsString};
use std::process::{Command, ExitCode, Stdio};

pub const VIBE_ACP_PATH_ENV: &str = "CADENCR_VIBE_ACP_PATH";
const DEFAULT_VIBE_ACP: &str = "vibe-acp";

fn vibe_acp_binary() -> OsString {
    std::env::var_os(VIBE_ACP_PATH_ENV).unwrap_or_else(|| DEFAULT_VIBE_ACP.into())
}

fn vibe_acp_command(binary: &OsStr) -> Command {
    let mut command = Command::new(binary);
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command
}

pub fn exec_vibe_acp(provider_args: &[OsString]) -> Result<ExitCode, String> {
    let binary = vibe_acp_binary();
    let mut command = vibe_acp_command(&binary);
    command.args(provider_args);
    run(command, &binary)
}

#[cfg(unix)]
fn run(mut command: Command, binary: &OsStr) -> Result<ExitCode, String> {
    use std::os::unix::process::CommandExt;
    // `exec` only returns on failure. On success this process image is
    // replaced by `vibe-acp`, which then owns the inherited stdio and the pid
    // CadencR signals.
    Err(launch_error(command.exec(), binary))
}

#[cfg(not(unix))]
fn run(mut command: Command, binary: &OsStr) -> Result<ExitCode, String> {
    // Windows has no `exec`, so the connector stays the parent and CadencR's
    // kill reaches the connector only. A managed install's job object still
    // tears the agent down; an imported one leaves it running.
    let status = command
        .status()
        .map_err(|error| launch_error(error, binary))?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .map(ExitCode::from)
        .unwrap_or(ExitCode::FAILURE))
}

fn launch_error(error: std::io::Error, binary: &OsStr) -> String {
    format!(
        "failed to start `{}`: {error}. Install the Mistral Vibe CLI \
         (https://mistral.ai/vibe) or set {VIBE_ACP_PATH_ENV}",
        binary.to_string_lossy()
    )
}

#[cfg(test)]
mod tests {
    use super::{exec_vibe_acp, VIBE_ACP_PATH_ENV};

    #[cfg(unix)]
    #[test]
    fn exec_reports_a_missing_binary_with_the_install_hint() {
        let missing = "/nonexistent/vibe-acp-for-test";
        std::env::set_var(VIBE_ACP_PATH_ENV, missing);
        let error = exec_vibe_acp(&[]).expect_err("exec of a missing binary must fail");
        std::env::remove_var(VIBE_ACP_PATH_ENV);
        assert!(error.contains(missing), "{error}");
        assert!(error.contains("https://mistral.ai/vibe"), "{error}");
        assert!(error.contains(VIBE_ACP_PATH_ENV), "{error}");
    }
}
