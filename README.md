# ArtCraft MCP Server & Automation Suite

[![Protocol: MCP](https://img.shields.io/badge/MCP-2026--07--28-blue.svg)](https://modelcontextprotocol.io)
[![Tools: 99](https://img.shields.io/badge/Tools-99-brightgreen.svg)](#-tool-catalog-99-tools)
[![Conformance: 40/40](https://img.shields.io/badge/Conformance-40%2F40-brightgreen.svg)](docs/verification-evidence-report.md)
[![Language: Rust](https://img.shields.io/badge/Language-Rust%202021-orange.svg)](https://www.rust-lang.org)
[![Python: 3.10+](https://img.shields.io/badge/Python-3.10%2B-green.svg)](https://python.org)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](./LICENSE.md)

Standalone repository bundling the **ArtCraft Model Context Protocol (MCP) server** — a Rust implementation of the current MCP specification revision (`2026-07-28`) with full backwards compatibility for `2024-11-05` clients — plus automation CLI tools, agent skills, JSON schemas, verification harnesses, and companion bridge routes for the **ArtCraft** generative media platform.

It lets AI coding assistants (Claude Desktop, Windsurf, Cursor, Antigravity, OpenClaw) generate images, videos, 3D meshes, Gaussian splats, and audio; manage the complete media library and folder organization system; and administer account, credential, billing, and community features through natural language or scripted calls.

---

## 🚀 Why this server is different

The capabilities below remain rare across MCP servers — and every one is exercised end-to-end in the committed [verification evidence report](docs/verification-evidence-report.md).

| Capability | What it actually does |
|---|---|
| **Current protocol: `2026-07-28`** | Negotiates the current MCP spec revision with modern clients while `2024-11-05` clients connect unchanged and receive the full 99-tool catalog. |
| **40/40 official conformance** | Passes all 40 checks across all 30 scenarios of the official `@modelcontextprotocol/conformance` active suite, with an **empty expected-failure baseline** (`server: []`). |
| **Progressive tool discovery** | Modern clients get a curated hot catalog plus `search` / `describe` / `call_deferred` meta-tools — 10 tools total when tasks support is declared — instead of a 99-tool firehose, with `ttlMs`/`cacheScope` caching hints. |
| **MRTR cost confirmation** | Billable tools return `input_required` with an HMAC-sealed `requestState` and an elicitation form. One-character tampering is rejected; declining spends zero credits; retries can't double-spend. |
| **Tasks extension** | Long-running generation jobs become real MCP tasks (`io.modelcontextprotocol/tasks`): enqueue → `tasks/get` → completed with the upstream result token. |
| **Files extension** | Direct upload-target negotiation (`io.modelcontextprotocol/files`): inline base64 for small files, out-of-band `PUT` targets up to 512 MiB, download targets in result metadata. |
| **MCP Apps** | Four embedded HTML UIs (`ui://artcraft/*`): media gallery, media player, 3D/splat viewer, and a live credit dashboard — served with strict CSP metadata. |
| **Dual transports** | stdio plus Streamable HTTP on `/mcp` with `/health`, Origin and Host allowlists (hostile `Host` header → 403), and optional bearer gating. |
| **Evidence-grade engineering** | Budget-guarded 99-tool E2E matrix, credit-ledger reconciliation, and a permanent upstream drift checker (94 usages matched, 0 drift against 364 upstream routes). |

The honest tradeoffs are documented too: three upstream generation endpoints return reproducible HTTP 500s, the retired voice family was **removed** rather than faked, and the deprecated TTS tool tells the model exactly why it can't enqueue work. See [⚠️ Known upstream issues](#️-known-upstream-issues).

---

## 📁 Project Structure

```
artcraft-mcp/
├── bin/
│   └── artcraft-mcp.exe                # The single v2.0.0 server — 99 tools, dual transport
│
├── rust/
│   └── artcraft_mcp_cli/               # Standalone Rust crate (artcraft-mcp v2.0.0)
│       ├── Cargo.toml                  # Upstream crates pinned by git rev; no monorepo needed
│       ├── src/
│       │   ├── main.rs                 # CLI: --transport, --tool-mode, MRTR thresholds
│       │   ├── server.rs               # Dual transport, MRTR, tasks, files, progressive discovery
│       │   ├── registry.rs             # Per-tool metadata: category, cost tier, async flag, tags
│       │   ├── discovery.rs            # search/describe/call_deferred meta-tools
│       │   ├── files_ext.rs            # io.modelcontextprotocol/files upload/download targets
│       │   ├── apps.rs                 # Four embedded MCP App HTML UIs
│       │   ├── auth.rs, client.rs      # Credential chain + strict-wins bearer/cookie auth
│       │   └── tools/                  # 20 tool modules covering the platform surface
│       └── tests/protocol_smoke.rs     # Protocol integration tests (legacy + modern)
│
├── tools/                              # Python automation & verification suite
│   ├── artcraft_runner.py              # Batch video & image generator with async polling
│   ├── artcraft_3d_runner.py           # Hunyuan 3D & WorldLabs Marble splat pipeline
│   ├── artcraft_angle_matrix.py        # 4-quadrant camera orbit generator
│   ├── artcraft_audio_weaver.py        # Audio stem reference & voice synthesis weaver
│   ├── artcraft_cost_estimator.py      # Live credit balance inspector & cost estimator
│   ├── artcraft_healthcheck.py         # Protocol/auth/cost smoke test
│   ├── artcraft_e2e_matrix.py          # Budget-guarded 99-tool end-to-end matrix
│   ├── artcraft_drift_check.py         # Upstream route drift checker
│   └── artcraft_*_evidence.py          # Retained/supplemental/transport/MRTR evidence collectors
│
├── skills/artcraft-mcp/                # Agent skill + model/aspect-ratio reference tables
├── companion/                          # FastAPI bridge: /api/mcp/{health,start,stop,token}
├── schemas/                            # 99 JSON-RPC tool definitions synced from tools/list
├── conformance/                        # conformance-baseline.yml (empty — 0 expected failures)
├── docs/                               # Verification evidence report + upstream recon notes
│
├── ArtCraft-MCP-Setup-Guide.md         # Full client setup guide (Claude, Windsurf, Cursor)
├── TOOLS.md / DEV-NOTES.md             # CLI docs / development notes
├── CHANGELOG.md                        # Release history
└── setup_mcp.py                        # 1-click MCP client configuration utility
```

---

## ⚡ Quick Start

### 1. One-Click Setup (Automatic)

Run the included configuration script to auto-detect and register `artcraft-mcp` across Claude Desktop, Windsurf, or Cursor:

```powershell
python setup_mcp.py
```

### 2. Manual Configuration

Add to your MCP client configuration file (e.g., `claude_desktop_config.json` or `~/.codeium/windsurf/mcp_config.json`):

```json
{
  "mcpServers": {
    "artcraft": {
      "command": "C:\\path\\to\\artcraft-mcp\\bin\\artcraft-mcp.exe",
      "args": [],
      "description": "ArtCraft AI generative media platform (99-tool engine)"
    }
  }
}
```

No arguments are required for stdio. Credentials resolve automatically (see [🔑 Authentication](#-authentication)).

---

## 🧠 Protocol & Architecture

### Protocol negotiation

The server advertises `2026-07-28` and negotiates down cleanly: a client that only speaks `2024-11-05` gets `2024-11-05` and all 99 tools, exactly as before. Modern clients unlock the progressive catalog, MRTR, tasks, files, and apps surfaces described below.

### Progressive tool discovery

On `2026-07-28` clients (`--tool-mode progressive`, the default), `tools/list` returns a curated hot catalog — the generators, credit/cost checkers, job pollers, and media download — plus three meta-tools that reach the rest of the catalog on demand:

| Meta-tool | Purpose |
|---|---|
| `artcraft_search_tools` | Token-weighted search over names, categories, tags, and descriptions (`names` / `summary` / `full` detail levels) |
| `artcraft_describe_tool` | Full input schemas, cost tier, and usage examples for 1–5 tools |
| `artcraft_call_deferred_tool` | Dispatch any non-hot tool, with schema-guided argument coercion (`"5"` → `5`, `"true"` → `true`) |

When the client declares tasks support, the redundant job-polling tools are hidden too, so the catalog shrinks to **10 tools**. `tools/list` responses carry `ttlMs=300000` and `cacheScope=private` hints. Legacy clients always get the flat 99-tool list; `--tool-mode full` forces it for everyone.

### MRTR cost confirmation

For billable tools on modern, elicitation-capable clients, the server interrupts the call with `resultType=input_required`: an HMAC-sealed `requestState` continuation plus a confirm/decline elicitation form quoting the credit cost. OmniGen generation tools are priced exactly — the gate calls the upstream `/v1/omni_gen/cost/*` route for the requested model and arguments, so free models like `flux_1_schnell` quote 0 credits — while tools without a matching cost route fall back to a conservative per-tier estimate labeled approximate.

- **Tamper-evident**: any modification to `requestState` (even one character) is rejected with `tampered or unknown request state`. Confirmations expire after 15 minutes.
- **Idempotent**: a consumed confirmation can't execute twice, so retried approvals never double-spend.
- **Honest decline**: declining or cancelling returns a clean "no credits spent" result — the upstream API is never called.
- **Automatic where supported**: legacy clients (no elicitation) bypass the gate and execute directly.

Tunables: `--confirm-over-credits N` (default `50`; `0` confirms every billable tool), `--auto-approve-under N` (default `0`), `--no-confirm` for headless CI.

### Tasks extension (`io.modelcontextprotocol/tasks`)

Async-job tools (generations, edits, animation, conversion) are promoted to real MCP tasks when the client declares tasks support: the call returns a task handle, `tasks/get` reports `working` → `completed` with both the enqueue result and the upstream result token, and `tasks/cancel` stops local tracking. Task cancellation is honest about its limit: the upstream API exposes no job-cancel endpoint, so cancelling a task does not claim to stop the remote job.

### Files extension (`io.modelcontextprotocol/files`)

File-taking tools accept three input forms: a host `file_path`, inline base64 (≤ 512 KiB, decoded and size-verified), or negotiated out-of-band upload — the server returns `input_required` carrying upload targets (`PUT`, ≤ 512 MiB) sealed inside the same `requestState` mechanism as MRTR, then re-verifies declared-vs-received bytes before executing. On stdio the upload targets live on an ephemeral loopback relay; on HTTP they are relative paths on the same listener. When the client accepts downloads, `artcraft_download_media_file` returns its URL as a download target in result metadata instead of inline text.

### MCP Apps (`io.modelcontextprotocol/apps`)

Four HTML views are compiled into the binary and served as `ui://` resources with `text/html;profile=mcp-app` and strict CSP metadata (`default-src 'none'`; media/images from `https:`/`data:` only):

| App URI | View |
|---|---|
| `ui://artcraft/media-gallery` | Grid gallery for browsing generated and uploaded media |
| `ui://artcraft/media-player` | Inline video/audio player |
| `ui://artcraft/viewer-3d` | 3D mesh and Gaussian splat preview |
| `ui://artcraft/credit-dashboard` | Live credit balance dashboard |

Tools whose results suit a view carry `_meta.ui.resourceUri`; hosts without apps support get plain text as before.

### Full spec surface

Prompts (5 cost-aware generation templates), resources (8: the four apps, two markdown guides, an example PNG, a machine-readable catalog index), RFC 6570 resource templates, resource subscriptions, `completion/complete`, `logging/setLevel`, and tool annotations (`readOnly`/`destructive`/`idempotent`/`openWorld`) are all implemented — which is what the 40/40 conformance result actually covers.

### Dual transports

| Transport | How to run | Notes |
|---|---|---|
| **stdio** (default) | `artcraft-mcp.exe` | Newline-delimited JSON-RPC; logs on stderr only. File uploads use an ephemeral loopback relay. |
| **Streamable HTTP** | `artcraft-mcp.exe --transport http --bind 127.0.0.1:8787` | Serves `/mcp` (legacy session mode + JSON responses) and `GET /health`. |

HTTP hardening is on by default: the `Origin` header must be absent or loopback (`Host` header allowlisting likewise rejects `evil.example` with 403), non-loopback binds are refused without `--allow-remote`, remote binds additionally require `ARTCRAFT_MCP_BEARER_TOKEN`, and host `file_path` arguments are disabled for remote-served sessions unless `ARTCRAFT_MCP_ALLOW_HOST_PATHS=1`.

### CLI flags

| Flag | Default | Purpose |
|---|---|---|
| `--transport stdio\|http` | `stdio` | Transport to serve |
| `--bind ADDR` | `127.0.0.1:8787` | HTTP bind address |
| `--tool-mode progressive\|full\|codemode` | `progressive` | Catalog exposure (progressive applies to `2026-07-28` clients only) |
| `--confirm-over-credits N` | `50` | MRTR threshold; `0` confirms every billable tool |
| `--auto-approve-under N` | `0` | Auto-approve quotes at or under N credits |
| `--no-confirm` | off | Disable MRTR entirely (headless CI) |
| `--allow-remote` | off | Permit non-loopback HTTP binds (requires bearer token) |

---

## 🔑 Authentication

Upstream ArtCraft credentials resolve once at startup, in this order:

1. **Environment variables**: `ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN`, then `ARTCRAFT_API_KEY`, then `ARTCRAFT_SESSION` with optional `ARTCRAFT_AVT`.
2. **Credential files** in `~/Artcraft/credentials/` (`%USERPROFILE%\Artcraft\credentials\` on Windows):
   - `artcraft_session.txt` — session cookie
   - `artcraft_avt.txt` — visitor cookie
3. **ArtCraft Desktop App cookie store**, for whichever cookie is still missing:

   | OS | Path |
   |---|---|
   | Windows | `%LOCALAPPDATA%\ai.artcraft.app\.cookies` |
   | macOS | `~/Library/Application Support/ai.artcraft.app/.cookies` |
   | Linux | `$XDG_DATA_HOME/ai.artcraft.app/.cookies` (default `~/.local/share/ai.artcraft.app/.cookies`) |

MCP private session tokens (`mcp_session_...`) and API keys (`artcraft_api_...`) are sent upstream as `Authorization: Bearer <secret>` — the header strictly wins and upstream does not fall back to cookies when it is present. Only the legacy web session pair is sent as cookies.

Server-side env vars:

| Variable | Purpose |
|---|---|
| `ARTCRAFT_MCP_BEARER_TOKEN` | Require this bearer on the HTTP transport (mandatory for `--allow-remote`) |
| `ARTCRAFT_MCP_STATE_SECRET` | requestState signing key; required for multi-instance HTTP deployments (random per-process otherwise) |
| `ARTCRAFT_MCP_ALLOW_HOST_PATHS` | Re-enable host `file_path` arguments while serving remote clients |

Without credentials the server still starts, serves discovery, and lists its tools; account-bound calls return an upstream authentication error.

---

## 🧰 Tool Catalog (99 tools)

Business tools by family. In progressive mode the three meta-tools (`artcraft_search_tools`, `artcraft_describe_tool`, `artcraft_call_deferred_tool`) are also exposed; legacy clients see the flat 99-tool list.

| Family | Tools |
|---|---|
| **Image generation & editing** (5) | `artcraft_generate_image`, `artcraft_edit_image`, `artcraft_edit_image_angle`, `artcraft_inpaint_image`, `artcraft_remove_background` |
| **Video** (2) | `artcraft_generate_video`, `artcraft_edit_video` |
| **Audio & TTS** (4) | `artcraft_generate_audio`, `artcraft_upload_audio`, `artcraft_tts_search_models`, `artcraft_tts_generate` †deprecated |
| **3D & Gaussian splats** (2) | `artcraft_generate_3d_object`, `artcraft_generate_splat` |
| **Cost & model catalog** (4) | `artcraft_estimate_cost`, `artcraft_estimate_splat_cost`, `artcraft_list_image_models`, `artcraft_list_video_models` |
| **Media library** (9) | `artcraft_upload_image`, `artcraft_upload_video`, `artcraft_download_media_file`, `artcraft_get_media_file`, `artcraft_list_media_files`, `artcraft_search_media`, `artcraft_rename_media_file`, `artcraft_set_media_visibility`, `artcraft_delete_media_file` |
| **Folders** (16) | `artcraft_list_folders`, `artcraft_create_folder`, `artcraft_get_folder`, `artcraft_delete_folder`, `artcraft_rename_folder`, `artcraft_set_folder_star`, `artcraft_set_folder_color`, `artcraft_set_folder_cover_image`, `artcraft_list_subfolders`, `artcraft_add_subfolders`, `artcraft_remove_subfolders`, `artcraft_list_folder_media_files`, `artcraft_list_media_files_without_folder`, `artcraft_add_media_to_folder`, `artcraft_remove_media_from_folder`, `artcraft_move_media_to_folder` |
| **Jobs** (3) | `artcraft_get_job_status`, `artcraft_list_jobs`, `artcraft_terminate_job` |
| **Characters** (4) | `artcraft_create_character`, `artcraft_list_characters`, `artcraft_get_character`, `artcraft_delete_character` |
| **Prompts** (2) | `artcraft_create_prompt`, `artcraft_get_prompt` |
| **Weights / LoRA** (4) | `artcraft_list_weights`, `artcraft_get_weight`, `artcraft_search_weights`, `artcraft_delete_weight` |
| **Community & social** (11) | Bookmarks: `artcraft_create_bookmark`, `artcraft_list_bookmarks`, `artcraft_delete_bookmark` · Comments: `artcraft_create_comment`, `artcraft_list_comments`, `artcraft_delete_comment` · `artcraft_rate_media` · Tags: `artcraft_set_tags`, `artcraft_list_tags` · Referrals: `artcraft_create_referral_code`, `artcraft_list_referral_codes` |
| **Featured** (3) | `artcraft_list_featured_media`, `artcraft_search_featured_media`, `artcraft_is_featured_item` |
| **Account & billing** (10) | `artcraft_get_session_info`, `artcraft_get_credits`, `artcraft_get_subscription`, `artcraft_get_billing_portal_url`, `artcraft_get_billing_cancel_plan_url`, `artcraft_get_billing_switch_plan_url`, `artcraft_get_billing_update_payment_method_url`, `artcraft_set_unsubscribe_reason`, `artcraft_create_checkout_session`, `artcraft_create_subscription_checkout` |
| **API keys** (5) | `artcraft_create_api_key`, `artcraft_list_api_keys`, `artcraft_get_api_key`, `artcraft_update_api_key`, `artcraft_delete_api_key` |
| **MCP sessions** (5) | `artcraft_create_mcp_session`, `artcraft_list_mcp_sessions`, `artcraft_refresh_mcp_session`, `artcraft_revoke_mcp_session`, `artcraft_delete_mcp_session` |
| **Studio** (2) | `artcraft_upload_studio_scene_snapshot`, `artcraft_upload_studio_shot` |
| **Animation & conversion** (5) | `artcraft_create_face_animation`, `artcraft_create_rerender_animation`, `artcraft_convert_fbx_to_gltf`, `artcraft_convert_render_engine_scene`, `artcraft_convert_bvh_to_workflow` |
| **Craft apps & beta** (3) | `artcraft_get_craft_apps_release_info`, `artcraft_list_beta_keys`, `artcraft_redeem_beta_key` |

Model coverage is tracked in `skills/artcraft-mcp/references/` — image (Flux, Flux Pro, Nano Banana Pro, Seedream, Midjourney, GPT Image), video (Kling, Veo, Seedance, Sora, Wan, MiniMax, Grok Imagine), 3D (Hunyuan 3D, Tripo3D, Meshy, Rodin), and splats (WorldLabs Marble, TripoSplat).

---

## 🍎 macOS & Linux

The crate builds standalone — upstream ArtCraft crates are pinned git dependencies, so no monorepo checkout is needed:

```bash
cd rust/artcraft_mcp_cli
cargo build --release
# binary: target/release/artcraft-mcp
```

The `Build binaries` GitHub Actions workflow produces macOS (Apple Silicon), Linux, and Windows artifacts. Register the binary with your client, e.g. Claude Code:

```bash
claude mcp add artcraft -- /path/to/artcraft-mcp
```

---

## 🛠️ CLI Automation Suite

All scripts in `tools/` drive `bin/artcraft-mcp.exe` over stdio JSON-RPC (Python 3.10+, `PYTHONUTF8=1`):

```powershell
# Credits & generation costs
$env:PYTHONUTF8="1"; python tools/artcraft_cost_estimator.py --check-balance --video-model seedance_2p0 --duration 5

# Batch image & video generation
$env:PYTHONUTF8="1"; python tools/artcraft_runner.py `
  --prompt "Cinematic cyberpunk alleyway at dusk, anamorphic lens, neon reflections" `
  --model flux_1_dev --copies 2 --out-dir "./outputs"

# Hunyuan 3D mesh & Gaussian splat generation
$env:PYTHONUTF8="1"; python tools/artcraft_3d_runner.py --image "path/to/character.png" --mode both --out-dir "./outputs/3d"

# 4-quadrant camera orbit pass
$env:PYTHONUTF8="1"; python tools/artcraft_angle_matrix.py --image "path/to/subject.png" --prompt "Maintain facial details during camera orbit" --out-dir "./outputs/orbit"
```

---

## ✅ Verification & Evidence

The full evidence base is committed at [docs/verification-evidence-report.md](docs/verification-evidence-report.md) (generated 2026-11-26 against the release binary). Headline results:

| Check | Result |
|---|---|
| Official MCP conformance (active suite) | **40/40 checks, 30/30 scenarios, empty baseline** |
| Business-tool E2E matrix | **93 PASS / 4 FAIL / 2 SKIP of 99** — all four FAILs are upstream HTTP 500s, not MCP bugs |
| Upstream drift check | **94 usages matched, 0 missing, 0 verb mismatches** vs 364 upstream routes |
| Spend ledger | **56 credits exact** — every measured call reconciles to banked-credit deltas |
| Retained artifacts | 9 real generated files (images, MP4, GLB) with media/job tokens |

Reproduce locally:

```powershell
# Protocol/auth/cost smoke
$env:PYTHONUTF8="1"; python tools/artcraft_healthcheck.py

# Zero-cost end-to-end matrix (validation & read-only probes)
$env:PYTHONUTF8="1"; python tools/artcraft_e2e_matrix.py --phases free

# Budgeted full matrix — spends real credits, capped
$env:PYTHONUTF8="1"; python tools/artcraft_e2e_matrix.py --phases all --budget-credits 150

# Upstream route drift check (needs a local artcraft-services clone; see AGENTS.md)
$env:PYTHONUTF8="1"; python tools/artcraft_drift_check.py --no-write-route-table

# Official conformance suite
./bin/artcraft-mcp.exe --transport http --bind 127.0.0.1:8787
npx -y @modelcontextprotocol/conformance server --url http://127.0.0.1:8787/mcp --suite active --expected-failures conformance/conformance-baseline.yml
```

---

## ⚠️ Known upstream issues

These are upstream ArtCraft API behaviors, not MCP-layer bugs — reproduced identically by direct API probing. Failed calls measured zero net credit deduction. Evidence for each is in the verification report.

| Surface | Status |
|---|---|
| `artcraft_inpaint_image`, `artcraft_generate_splat`, `artcraft_edit_video` | Upstream returns HTTP 500 with valid inputs before a job token is produced. Reproducible; costs 0 credits. |
| `artcraft_generate_audio` (Suno) | Upstream job scheduling stall: enqueue charged 3 credits, then the job sat `pending` at 0% for 600+ seconds in two independent runs. |
| `artcraft_create_character` | Intermittent upstream HTTP 500 under concurrent load; the independent create/poll/list/get/delete lifecycle passed cleanly. |
| `artcraft_tts_generate` | **Deprecated** — upstream `/v1/tts/inference` only returns a synthetic nonfunctional job token, so the tool explains rather than enqueueing fake work. |
| `artcraft_tts_search_models` | Works via the legacy `/tts/list` compatibility endpoint. |
| `artcraft_search_weights` | Upstream weight search is retired and may always return empty. |
| `artcraft_estimate_cost` | Supports image, video, audio, mesh, and splat via the upstream OmniGen cost routes. |
| Referral codes / prompts | No delete tools: referral deletion exists upstream but has no MCP wrapper; prompts have no upstream delete route at all. |
| `tasks/cancel` | Stops local tracking only — upstream exposes no job-cancel endpoint. |

The retired voice-conversion/voice-designer family was **removed** in v2.0 rather than kept as dead stubs, and a `studio_gen2` tool whose upstream route never existed was removed for the same reason.

---

## 🤖 AI Agent Skills

Agent skills and prompt guidance are provided in `skills/artcraft-mcp/`:
- **`SKILL.md`**: Behavioral instructions, parameter schemas, and error handling for LLM agents.
- **`references/`**: Model capability tables, aspect ratio rules, and duration limits for image, video, 3D/splat, and audio models.

---

## 🖥️ Companion API Bridge

For applications integrating the MCP server as a managed subprocess, `companion/mcp_routes.py` provides FastAPI endpoints:
- `GET /api/mcp/health` — Check process state and PID.
- `POST /api/mcp/start` — Spawn the binary with stdio pipes.
- `POST /api/mcp/stop` — Gracefully terminate or kill the process.
- `POST /api/mcp/token` — Exchange API keys/cookies for an MCP session token.

---

## 🔧 Development

```powershell
cd rust/artcraft_mcp_cli
cargo check --all-targets
cargo test --all-targets        # unit + protocol integration tests
cargo clippy --all-targets -- -D warnings
cargo build --release
```

CI runs the same gates plus a packaged-binary smoke test and the official conformance suite on every change to `rust/`, `bin/`, or `conformance/`.

---

## 📜 Documentation

- [Verification Evidence Report](docs/verification-evidence-report.md) — the machine-checkable basis for every claim above.
- [Setup Guide](ArtCraft-MCP-Setup-Guide.md) — Comprehensive client installation manual.
- [CLI Tools Guide](TOOLS.md) — Detailed parameters for all Python tools.
- [Developer Notes](DEV-NOTES.md) — Architecture and protocol details.
- [Rust Source Guide](rust/README.md) — Source code walkthrough.
- [Companion Guide](companion/docs_mcp_server.md) — FastAPI / FastMCP bridge architecture.
- [Upstream recon notes](docs/) — `craft-ai-integrations`/auth and `autocraft` architecture studies.
