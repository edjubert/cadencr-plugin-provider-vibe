# cadencr-plugin-provider-vibe

Cadencr code-backed provider connector for the [Mistral Vibe](https://github.com/mistralai/mistral-vibe) coding agent.

The connector implements the Cadencr provider executable contract:

| Command | Behavior |
| --- | --- |
| `version` | Prints the connector version. |
| `models --format acp-config-options-v1 --cwd <abs>` | Opens a disposable ACP session against `vibe-acp`, reads the live model selector from `session/new`, closes the session, and prints the ACP v1 `SessionConfigOption` array. |
| `run --protocol acp-v1` | Execs the native `vibe-acp` binary with stdin/stdout/stderr unchanged. Vibe speaks ACP v1 natively, so there is no translation layer. |

## Prerequisites

Authentication is handled entirely by the native Vibe CLI, per the Cadencr
connector contract. Before installing or running the connector:

```bash
# either sign in / store credentials in ~/.vibe/.env
vibe --setup

# or export your key in the environment
export MISTRAL_API_KEY=...
```

The connector and Cadencr never collect, store, or broker Vibe credentials.

## Install the Vibe CLI

```bash
curl -LsSf https://mistral.ai/vibe/install.sh | bash
# or: uv tool install mistral-vibe
```

This installs `vibe`, `vibe-acp`, and `vibe-app-server`. The connector execs
`vibe-acp`.

## Build

```bash
cargo build --release
```

## Binary resolution

`vibe-acp` is resolved in this order:

1. `CADENCR_VIBE_ACP_PATH` environment variable (absolute path recommended);
2. `vibe-acp` on `PATH`.

## Local Cadencr install

1. Copy `descriptor/mistral-vibe.json` to `~/.cadencr/settings/providers/mistral-vibe.json`.
2. Replace both `/ABSOLUTE/PATH/TO/...` placeholders with this repository's absolute path.
3. Restart Cadencr. Provider registration is read once at startup; hot reload is not supported.

To install from another directory, keep `installation.executable.command`
pointing at the release binary and `installation.assets.directory` at this
repository root (the directory that holds `icon.svg`).

## Notes

- Model discovery uses a real disposable ACP session so the pre-session
  selector always matches the live session selector (same binary, same ids).
  Discovery stays under Cadencr's 10-second host timeout with an internal
  8-second budget.
- Vibe releases are frequent (several per week). The connector only relies on
  stable ACP v1 surface (`initialize`, `session/new`, `session/close`,
  `configOptions`), so a CLI upgrade does not require a connector change.
- The `thinking` config option Vibe exposes uses a non-standard category
  (`thinking` instead of `thought_level`). It appears in Cadencr as a generic
  selector, not as the native thinking-effort control.
- Durable resume: Vibe advertises `loadSession` (legacy load with replay
  suppression on the Cadencr side) and `session/close`. Cadencr handles both
  automatically.
- Publication to the Cadencr registry (signed package via `pack-provider`)
  is a later step; the registry contribution gate is not open yet.
