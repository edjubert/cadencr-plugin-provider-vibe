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

## CadencR v0.16.0 Marketplace preflight — 2026-11-03

Checked the published CadencR `v0.16.0` provider contract and built its
`cadencr` CLI (`0.15.4`) from that exact source tag.

| Check | Result |
| --- | --- |
| Native release binary | Apple Silicon macOS build reports `0.1.0`. |
| Model discovery | Vibe `2.26.0` returns 19 choices, with a valid current model. |
| ACP runtime | ACP v1 `initialize`, `session/new`, and `session/close` pass; the discovered model selector matches the live session selector. |
| Marketplace package | `pack-provider` creates an archive containing `bin/provider`, `icon.svg`, `README.md`, and `LICENSE`; the executable is mode `0755`. The extracted archive passes local provider validation, model discovery, and ACP v1 lifecycle checks. |
| Archive digest | `5c6cd252bef4fb21acc584a145f8d65df6a560b4169f5bbd8b417edc19c75bce` (`darwin-aarch64`); a repeated pack produces identical bytes. |
| CadencR validation | Schema-2 package and submission pass `registry validate-metadata` and `registry validate-submission`; a candidate tree passes `registry validate` against the registry's current `main`. |
| Unit tests | All eight Rust tests pass. |

An Intel macOS binary was also cross-built and exercised under Rosetta. It is
not declared in the Marketplace metadata because that was not a native Intel
host qualification. Linux and Windows targets have not been qualified.

The initial registry preflight used a placeholder source commit. The actual
released source binding and uploaded bytes were checked separately below.

## Author release and registry submission — 2026-11-03

| Check | Result |
| --- | --- |
| Released source | The immutable `v0.1.0` tag resolves to `a1619e77dfb09a8b2742e3dc6aa5cf5362fe69a6`, the merged source revision on `main`. |
| Author publication | The shared `cadencr plugin inspect-release` and `publish-release` engine published the [GitHub release](https://github.com/edjubert/cadencr-plugin-provider-vibe/releases/tag/v0.1.0), with request SHA-256 `b02651900f9e5984ce4cda40899a7fcc8a5c75e484e263020dff296380f551a0`. |
| Uploaded bytes | The downloaded archive has SHA-256 `5c6cd252bef4fb21acc584a145f8d65df6a560b4169f5bbd8b417edc19c75bce`; the downloaded `package.json` is byte-identical to the source metadata, with SHA-256 `101eb17740f2bbb5e980dc27dbd9ca8a2b086c0a9b5e531f2a8913996d56cbe4`. |
| Pinned registry CLI | The official Linux `cli/v0.15.4` executable matches the registry pin `d925f203c3fd2e51621ad00ef1413bb25fdf8bc25792dbec9d5d2a67b1d4244d`. Its metadata, submission and complete contribution validators pass in an Ubuntu 24.04 container with read-only inputs. |
| Registry contribution | [Registry PR #21](https://github.com/merkr-software/cadencr-registry/pull/21) adds the matching schema-2 package and submission, bound to the released tag and full source commit. Validation used registry base `e31b07eea3d17cc501f8e8b607784ebc1c6c98b0`. |
| Registry CI | [Workflow run 41689046091](https://github.com/merkr-software/cadencr-registry/actions/runs/41689046091) subsequently passed both `inert-metadata` and `contributor-tooling` after maintainer approval. These checks cover the original `0.1.0` candidate. |

## Registry review corrections — 2026-11-04

Addressed the two discovery defects reported in
[registry PR #21](https://github.com/merkr-software/cadencr-registry/pull/21).
The replacement release is `v0.1.1`; the existing `v0.1.0` tag and release
assets are preserved.

| Check | Result |
| --- | --- |
| Child lifetime | A process guard takes ownership immediately after spawn. Every subsequent exit, including initialization failure and timeout, kills and reaps the disposable ACP child. |
| Global discovery deadline | Initialization and session creation share one deadline established before spawn. The total budget is 9 seconds, including up to 1 second reserved for session cleanup, below the host's 10-second timeout. |
| Regression reproduction | All four new regression tests fail against the prior implementation: failed initialization, timed-out initialization, a 7-second initialization followed by 4-second session creation, and an unresponsive session close. |
| Fixed regressions | All 13 Rust tests pass locally and in the native Apple Silicon workflow. The process tests check `waitpid` returns `ECHILD`, proving the child has been reaped, and enforce elapsed-time bounds. |
| Native build provenance | `Native provider build` tests and builds the exact source revision on an ARM64 macOS runner with Rust `1.97.1` and the locked dependencies. Its unchanged `provenance.json` is published beside the release archive and metadata, recording the source commit, successful workflow run, toolchain, and each distribution file's SHA-256 and size. |
| CI executable | The downloaded CI executable is native ARM64, passes strict code-signature verification, and reports `0.1.1`. Its SHA-256 is `2f7e14096db85d6b0ae7dd7b03b17b520ba0e239d09e4048ed2da5718d5fe7f3`. |
| Real Vibe integration | With Vibe `2.26.0`, the CI executable discovers 19 models in 1.9 seconds, with a valid current model. ACP v1 initialization, session creation, and session closure pass; the discovered selector matches the live runtime. No prompt is sent. |
| Replacement archive | The CadencR `0.15.4` CLI built from the exact `v0.16.0` application tag packs the CI distribution. The archive's SHA-256 is `7c96a1940f9e3b02a8c4a9f9eb9b803a28d20423a26d8f2e3a5a111195645ba4`; it contains only `bin/provider` (mode `0755`), `icon.svg`, `README.md`, and `LICENSE`. |

The published provenance receipt identifies the build for the exact tagged
source revision. Registry CI validates the final replacement candidate with
the registry's pinned official Linux CLI.

The CadencR v0.16.0 desktop Marketplace installation flow has not been exercised.
Marketplace listing still requires maintainer acceptance, protected mirroring,
and signed catalog publication.
