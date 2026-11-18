//! `run --protocol acp-v1`: hand the process over to the native `vibe-acp` binary.
//!
//! Vibe speaks ACP v1 natively, so the connector adds no translation layer.
//! The binary is resolved from `CADENCR_VIBE_ACP_PATH`, defaulting to
//! `vibe-acp` on `PATH`. Environment is inherited unchanged so the native
//! CLI keeps its own credentials (`~/.vibe/.env` or `MISTRAL_API_KEY`).
//!
//! The handover replaces this process image on Unix rather than spawning a
//! child. Vibe holds a per-session lease (`~/.vibe/logs/session/active/<id>.lock`)
//! for as long as the agent runs, and the host stops a session by signalling
//! the provider process. A wrapper in between takes that signal and leaves
//! `vibe-acp` orphaned with the lease still held, so the next `session/load`
//! for that session fails with `Session is already open`. Replacing the image
//! keeps one process per session: the signal reaches the agent, and the lease
//! dies with it.

use std::ffi::{OsStr, OsString};
use std::process::{Command, ExitCode, Stdio};

pub const VIBE_ACP_PATH_ENV: &str = "CADENCR_VIBE_ACP_PATH";
const DEFAULT_VIBE_ACP: &str = "vibe-acp";

pub fn exec_vibe_acp(provider_args: &[OsString]) -> Result<ExitCode, String> {
    let binary = std::env::var_os(VIBE_ACP_PATH_ENV).unwrap_or_else(|| DEFAULT_VIBE_ACP.into());
    let mut command = Command::new(&binary);
    command
        .args(provider_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    hand_over(command, &binary)
}

/// Replace this process with `command`, or wait for it where the platform
/// cannot replace the image.
#[cfg(unix)]
fn hand_over(mut command: Command, binary: &OsStr) -> Result<ExitCode, String> {
    use std::os::unix::process::CommandExt;

    // `exec` returns only when the handover failed.
    Err(start_error(binary, command.exec()))
}

#[cfg(not(unix))]
fn hand_over(mut command: Command, binary: &OsStr) -> Result<ExitCode, String> {
    let status = command
        .status()
        .map_err(|error| start_error(binary, error))?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .map(ExitCode::from)
        .unwrap_or(ExitCode::FAILURE))
}

fn start_error(binary: &OsStr, error: std::io::Error) -> String {
    format!(
        "failed to start `{}`: {error}. Install the Mistral Vibe CLI \
         (https://mistral.ai/vibe) or set {VIBE_ACP_PATH_ENV}",
        binary.to_string_lossy(),
    )
}

#[cfg(test)]
mod tests {
    use super::{hand_over, VIBE_ACP_PATH_ENV};
    use std::ffi::OsStr;
    use std::process::Command;

    #[test]
    fn a_missing_binary_reports_the_install_hint() {
        let binary = OsStr::new("/nonexistent/cadencr-vibe-test/vibe-acp");
        let error = hand_over(Command::new(binary), binary).unwrap_err();
        assert!(error.contains("failed to start"), "{error}");
        assert!(error.contains(VIBE_ACP_PATH_ENV), "{error}");
    }

    /// The handover must leave one process, not a wrapper waiting on a child:
    /// the host stops a session by signalling the provider, and a surviving
    /// `vibe-acp` keeps the session lease that the next load needs.
    #[cfg(unix)]
    #[test]
    fn the_handover_replaces_the_process_image() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runner::tests::fake_agent_process",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("CADENCR_VIBE_TEST_SCENARIO", "handover")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parent: u32 = stdout
            .lines()
            .find_map(|line| line.strip_prefix("ppid="))
            .expect("the reporter must print its parent")
            .trim()
            .parse()
            .unwrap();
        assert_eq!(
            parent,
            std::process::id(),
            "the handover left a wrapper between the host and the agent"
        );
    }

    /// Runs in an isolated copy of this test binary, without a CLI dependency.
    #[cfg(unix)]
    #[test]
    fn fake_agent_process() {
        let Ok(scenario) = std::env::var("CADENCR_VIBE_TEST_SCENARIO") else {
            return;
        };
        // End the test harness's prefix before writing the marker.
        println!();
        match scenario.as_str() {
            "handover" => {
                let mut command = Command::new(std::env::current_exe().unwrap());
                command
                    .args([
                        "--exact",
                        "runner::tests::fake_agent_process",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("CADENCR_VIBE_TEST_SCENARIO", "report-parent");
                let error = hand_over(command, OsStr::new("fixture")).unwrap_err();
                panic!("handover failed: {error}");
            }
            "report-parent" => {
                println!("ppid={}", unsafe { libc::getppid() });
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
            }
            _ => {}
        }
    }
}
