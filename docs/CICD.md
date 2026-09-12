# TeleStash CI/CD Pipeline

All Rust compilation, frontend bundling, installer creation, and release
publishing happen exclusively through GitHub Actions. **No build toolchain is
installed locally** — do not install MSVC, Rust, cargo, or any build toolchain
on this machine.

## Workflows

### CI (`ci.yml`)

Runs on every pull request and every push to `main`. Concurrency-capped (newer
runs cancel older ones on the same ref).

**Runner:** `windows-latest` (MSVC required for Rust compilation — CI provides
its own toolchain, it is NOT installed locally).

| Step | What it does | Failure output |
|---|---|---|
| Setup Node 26 + npm cache | Frontend toolchain | — |
| Install Rust MSVC toolchain | `stable-x86_64-pc-windows-msvc` via `dtolnay/rust-toolchain` | — |
| Rust cache | `Swatinem/rust-cache@v2` on `app/src-tauri -> target` | — |
| `npm ci` | Install frontend dependencies | — |
| Validate frontend (tsc) | `npx tsc --noEmit --pretty false` | `frontend-validation.log` |
| Run automated unit tests | `npm test` (4 suites, 15 tests) | `unit-tests-validation.log` |
| Provide MPV resource placeholder | Creates empty `bin/mpv-*.exe` so cargo check passes (CI does not bundle the real sidecar for validation) | — |
| Validate Rust source | `cargo check --locked --message-format short` | `rust-validation.log` |
| Run Rust unit tests | `cargo test --locked --lib` (32 tests) | `rust-tests.log` |
| Upload validation diagnostics | Only on failure; uploads all 4 log files as artifact | — |

On failure, GitHub annotations show the last 20–30 lines of each failed log
for immediate triage without downloading artifacts.

### Release (`release.yml`)

Triggered exclusively by pushing a `v*` tag (or manual `workflow_dispatch`).

**Pipeline:** 3 jobs in sequence.

#### Job 1: `validate-release`

Same validation as CI, plus:

| Step | What it does |
|---|---|
| Record validation context | Writes toolchain versions (Node, npm, rustc, cargo, tauri CLI) to the job summary |
| Validate release version and changelog | `npm run test:versions` (5-file version sync) + checks `## [x.y.z]` entry exists in `CHANGELOG.md` |
| Verify updater signing key | `npm run verify:updater-signing-key` — fails before build if `TAURI_SIGNING_PRIVATE_KEY` secret is absent or mismatched |

#### Job 2: `build-tauri`

| Step | What it does |
|---|---|
| Setup MPV sidecar | Downloads the real `mpv-x86_64-pc-windows-msvc.exe` (placeholder in CI is only for validation) |
| Prepare + verify updater signing key | Decrypts and validates the minisign key pair |
| Build Windows installer | `tauri build` — embeds frontend (vite build), compiles Rust (thin LTO, codegen-units=1), creates NSIS installer + updater bundle |
| Collect required release assets | Verifies `latest.json`, `setup.exe`, `.sig` all exist |
| Upload release assets | Uploads to the draft release |

Build time: 20–27 minutes (thin LTO + codegen-units=1 trade-off, agreed by user).

#### Job 3: `publish-release`

| Step | What it does |
|---|---|
| Download release assets | Pulls from the draft release |
| Publish verified release | Creates/updates the GitHub release, marks as published (not draft) |

## Required Secrets

| Secret | Purpose |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | Minisign private key for updater bundle signing |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Decryption password for the private key |

The workflow fails before building if the signing key is absent. Never add key
material to source code, logs, or commits.

## Diagnostic Artifacts

On any frontend, signing, Rust, or Tauri build failure, the workflow uploads a
downloadable artifact (`release-diagnostics-{run_id}` or `ci-diagnostics-{run_id}`)
containing the full captured command logs. The failing step also adds its final
log lines as a GitHub error annotation for immediate triage.

For temporary platform-level step tracing, set the repository Actions secret
`ACTIONS_STEP_DEBUG` to `true`, rerun the failed workflow, then remove the
secret. Do not enable it for routine releases and never print signing secrets.

## Why MSVC in CI but NOT locally

Rust compilation on Windows requires the MSVC linker and Windows SDK headers
(`x86_64-pc-windows-msvc` target). GitHub Actions `windows-latest` runners
provide these pre-installed. The local machine intentionally has no MSVC, no
Rust, and no cargo (removed 2026-09-10 to free ~11 GB disk). Do NOT install
them — all compilation is cloud-only.

## Release Checklist (abbreviated)

Full procedure: `docs/RELEASE_RUNBOOK.md`.

1. Obtain explicit user approval to commit, push, tag, and publish.
2. Bump version in 5 files: `app/package.json`, `app/package-lock.json`,
   `app/src-tauri/Cargo.toml`, `app/src-tauri/Cargo.lock`,
   `app/src-tauri/tauri.conf.json`.
3. Add nonempty `## [x.y.z]` entry to `CHANGELOG.md`.
4. Update `README.md` for material user-facing changes.
5. Validate locally: `npx tsc --noEmit --pretty false` + `npm test` +
   `git diff --check` from `C:\Telestash`.
6. Commit, push to `main`, create annotated tag `vX.Y.Z`, push tag.
7. Monitor the Release workflow until `publish-release` completes.
8. Verify assets: `gh release view vX.Y.Z` — `latest.json` + `.exe` + `.sig`
   must be present, `isDraft` must be `false`.
