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

The descriptor must be installed beside the development database, in
`packages/service/cadencr-settings/providers/` for the standard checkout.
Installing it at the Cadencr repository root does not register it under
`pnpm dev`. See the README for production installation.

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
