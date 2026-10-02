//! Cadencr connector package for the Mistral Vibe agent.
//!
//! Implements the Cadencr code-backed provider executable contract:
//! `models --format acp-config-options-v1 --cwd <absolute-path>`,
//! `run --protocol acp-v1`, and `version`.
//!
//! `run` execs the native ACP binary `vibe-acp` with stdio unchanged.
//! `models` opens a disposable ACP session against the same binary and
//! returns the live model selector observed at `session/new`, so discovery
//! and the live session always agree on option ids and values.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

mod discovery;
mod runner;

const MODEL_DISCOVERY_FORMAT: &str = "acp-config-options-v1";
const ACP_V1_PROTOCOL: &str = "acp-v1";

fn main() -> ExitCode {
    match execute(std::env::args_os().skip(1).collect()) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn execute(args: Vec<OsString>) -> Result<ExitCode, String> {
    let Some(command) = args.first().and_then(|value| value.to_str()) else {
        return Err("expected `models`, `run`, or `version`".to_string());
    };
    match command {
        "models" => execute_models(&args[1..]),
        "run" => execute_run(&args[1..]),
        "version" if args.len() == 1 => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(format!("unknown or invalid provider command `{command}`")),
    }
}

fn execute_models(args: &[OsString]) -> Result<ExitCode, String> {
    if args.len() < 4 || args[0] != "--format" || args[2] != "--cwd" {
        return Err(format!(
            "models requires `--format {MODEL_DISCOVERY_FORMAT} --cwd <absolute-path>`"
        ));
    }
    if args[1] != MODEL_DISCOVERY_FORMAT {
        return Err(format!("models requires `--format {MODEL_DISCOVERY_FORMAT}`"));
    }
    let cwd = PathBuf::from(&args[3]);
    if !cwd.is_absolute() {
        return Err("models `--cwd` must be absolute".to_string());
    }
    let options = discovery::discover_model_options(&cwd)?;
    serde_json::to_writer(std::io::stdout().lock(), &options).map_err(|e| e.to_string())?;
    println!();
    Ok(ExitCode::SUCCESS)
}

fn execute_run(args: &[OsString]) -> Result<ExitCode, String> {
    if args.len() < 2 || args[0] != "--protocol" || args[1] != ACP_V1_PROTOCOL {
        return Err(format!("run requires `--protocol {ACP_V1_PROTOCOL}`"));
    }
    runner::exec_vibe_acp(&args[2..])
}

#[cfg(test)]
mod tests {
    use super::execute;
    use std::ffi::OsString;
    use std::process::ExitCode;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn rejects_missing_and_non_absolute_model_arguments() {
        assert!(execute(args(&["models"])).is_err());
        assert!(execute(args(&[
            "models",
            "--format",
            "acp-config-options-v1",
            "--cwd",
            "relative",
        ]))
        .is_err());
    }

    #[test]
    fn rejects_unknown_format_and_protocol() {
        assert!(execute(args(&[
            "models",
            "--format",
            "other",
            "--cwd",
            "/tmp",
        ]))
        .is_err());
        assert!(execute(args(&["run", "--protocol", "acp-v2"])).is_err());
    }

    #[test]
    fn rejects_unknown_commands() {
        assert!(execute(args(&["frobnicate"])).is_err());
        assert!(execute(args(&["version", "extra"])).is_err());
    }
}
