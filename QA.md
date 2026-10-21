# Local integration validation

Validated on 2026-10-02 against Cadencr 0.12.0 using `pnpm dev`.

| Check | Result |
| --- | --- |
| Local provider registration | Provider appears as `@mistralai/mistral-vibe` with its icon. |
| Model discovery | Six configured Vibe models appear in the UI. |
| First prompt | `mistral-medium-3.5` returns the requested marker `VIBE_QA_OK_454`. |
| Resume with model change | `glm-5-3` recalls the same marker without it being repeated in the second prompt. |
| Durable session identity | Backend logs show a runtime respawn and the same ACP session ID before and after resume. |
| Persisted UI history | A separate Chrome instance displays both prompts and both responses. |
| Unit tests | All eight tests pass. |

That validation used a manually installed descriptor beside the development
database, in `packages/service/cadencr-settings/providers/` for the standard
checkout. Installing it at the Cadencr repository root does not register it
under `pnpm dev`. The current settings import flow registers the descriptor
automatically; see the README for its installation steps.

The integrated automation browser reported a zero-size viewport, preventing
virtualized chat history from rendering. Visual validation therefore used
a separate headless Chrome instance at 1440 × 1000.

Observed limitations outside these checks:

- Automatic conversation naming timed out; the conversation kept its default
  title.
- The browser console reported the existing CSP `frame-ancestors` meta-tag
  warning.
- Permission dialogs, file edits, cancellation and registry publication were
  not exercised.

No Cadencr application source changes were required.

## Settings import preparation — 2026-10-21

Validated on macOS against the existing-folder import requirements:

| Check | Result |
| --- | --- |
| Repository layout | Connector folder is the Git repository root. |
| Stable build output | `python3 scripts/build-provider.py` produces a regular executable at `bin/provider`. |
| Version entrypoint | `./bin/provider version` reports `0.1.0`. |
| Model discovery | `./bin/provider models --format acp-config-options-v1 --cwd "$PWD"` returns 13 distinct model choices in 5.4 seconds, with a valid current value. |
| Runtime entrypoint | `./bin/provider run --protocol acp-v1` completes native ACP v1 initialization over stdio. |
| Unit tests | All eight tests pass. |
| Git hygiene | `bin/provider` is ignored; only source and documentation changes are committed. |

The README now documents **Settings → Providers → Add provider → From existing
folder**, which automatically registers the local descriptor targeting
`bin/provider` and the root Mistral `icon.svg`. The manual descriptor template
uses the same stable executable path.

These checks cover the executable and import prerequisites. The settings
dialog itself was not exercised during this validation.
