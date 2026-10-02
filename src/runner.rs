//! `run --protocol acp-v1`: hand stdio to the native `vibe-acp` binary.
//!
//! Vibe speaks ACP v1 natively, so the connector adds no translation layer.
//! The binary is resolved from `CADENCR_VIBE_ACP_PATH`, defaulting to
//! `vibe-acp` on `PATH`. Environment is inherited unchanged so the native
//! CLI keeps its own credentials (`~/.vibe/.env` or `MISTRAL_API_KEY`).

use std::ffi::OsString;
use std::process::{Command, ExitCode, Stdio};

pub const VIBE_ACP_PATH_ENV: &str = "CADENCR_VIBE_ACP_PATH";
const DEFAULT_VIBE_ACP: &str = "vibe-acp";

fn vibe_acp_command() -> Command {
    let binary = std::env::var_os(VIBE_ACP_PATH_ENV).unwrap_or_else(|| DEFAULT_VIBE_ACP.into());
    Command::new(binary)
}

pub fn exec_vibe_acp(provider_args: &[OsString]) -> Result<ExitCode, String> {
    let status = vibe_acp_command()
        .args(provider_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| {
            format!(
                "failed to start `{}`: {error}. Install the Mistral Vibe CLI \
                 (https://mistral.ai/vibe) or set {VIBE_ACP_PATH_ENV}",
                std::env::var_os(VIBE_ACP_PATH_ENV)
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| DEFAULT_VIBE_ACP.to_string()),
            )
        })?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .map(ExitCode::from)
        .unwrap_or(ExitCode::FAILURE))
}
