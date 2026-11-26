# Changelog

All notable changes to the **ArtCraft Model Context Protocol (MCP) Server** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [2.0.0] - 2026-11-26

Major rebuild of the server and its verification story: the protocol moves from `2024-11-05` to the current `2026-07-28` revision, the catalog grows from 61 documented tools to 99 verified business tools, and the two-binary layout consolidates into a single dual-transport `artcraft-mcp`. The major bump is justified by breaking removals (upstream-retired capabilities) and the protocol/auth rework; legacy `2024-11-05` clients keep working unchanged and still receive the full catalog.

Verified against the release binary in `docs/verification-evidence-report.md`: **40/40** official conformance checks across **30/30** scenarios with an empty expected-failure baseline, **93 PASS / 4 FAIL / 2 SKIP** across all 99 tools (the failures are upstream HTTP 500s, not MCP bugs), **94 matched / 0 missing / 0 verb mismatches** against 364 upstream routes, and an exact 56-credit reconciled spend with 9 retained generated artifacts.

### Added
- **43 new business tools** covering the platform surface end to end:
  - **Folders** (16): list/create/get/delete/rename, star, color, cover image, subfolder add/list/remove, folder media list, unfoldered list, and media add/remove/move — the full organization system.
  - **API keys** (5): create/list/get/update/delete.
  - **MCP sessions** (5): create/list/refresh/revoke/delete — mint and manage MCP-scoped credentials through the tool surface itself.
  - **Studio** (2): scene-snapshot and shot uploads.
  - **Legacy animation** (2): face animation and rerender jobs.
  - **Conversion** (3): FBX→glTF, render-engine scene, and BVH→workflow jobs.
  - **Featured & beta** (5): featured media list/search/status, beta-key list/redeem.
  - **Billing** (4): cancel-plan, switch-plan, and update-payment-method portal URLs, plus unsubscribe-feedback submission.
  - **Craft apps** (1): public Craft-app release info.
  - **Social completion** (2): `artcraft_delete_bookmark`, `artcraft_delete_comment`.
- **Protocol `2026-07-28`**: the server advertises the current MCP revision; version negotiation keeps `2024-11-05` clients fully functional.
- **Progressive tool discovery**: `tools/list` on modern clients returns a hot catalog (the generators, credit/cost checkers, and job pollers; the two pollers are hidden when the client declares tasks support, shrinking the catalog to 10 tools) plus three meta-tools — `artcraft_search_tools`, `artcraft_describe_tool`, `artcraft_call_deferred_tool` — with `ttlMs`/`cacheScope` caching hints and canonical ordering. Controlled by `--tool-mode progressive|full|codemode`.
- **MRTR cost confirmation**: billable tools on modern elicitation-capable clients return `input_required` with an HMAC-sealed `requestState` and a confirm/decline elicitation form. The five OmniGen generation tools are priced exactly — the gate calls the upstream `/v1/omni_gen/cost/*` route for the requested model and arguments (free models quote 0); tools without a matching cost route get a conservative per-tier estimate labeled approximate. `--confirm-over-credits`, `--auto-approve-under`, and `--no-confirm` control the gate. Legacy clients execute directly.
- **Tasks extension** (`io.modelcontextprotocol/tasks`): async-job tools are promoted to MCP tasks with `tasks/get` and `tasks/cancel`; task completion embeds the upstream job result token.
- **Files extension** (`io.modelcontextprotocol/files`): inline base64 inputs (≤512 KiB), negotiated out-of-band `PUT` upload targets (≤512 MiB) carried inside the sealed `requestState` mechanism, byte re-verification, and download targets in result metadata; stdio serves uploads through an ephemeral loopback relay.
- **MCP Apps** (`io.modelcontextprotocol/apps`): four embedded HTML UIs — media gallery, media player, 3D/splat viewer, credit dashboard — served as `ui://` resources with strict CSP metadata; related tools annotate `_meta.ui.resourceUri`.
- **Streamable HTTP transport**: `/mcp` endpoint with legacy session mode and JSON responses, plus `GET /health`; stdio remains the default transport.
- **Full spec surface**: prompt-template library (5 cost-aware generation prompts), static and template resources with subscription tracking, `completion/complete`, `logging/setLevel`, and registry-derived tool annotations (`readOnly`/`destructive`/`idempotent`/`openWorld`).
- **New model ids**: `gpt_image_2p5_flare`/`gpt_image_2p5_sunburst` (image), `wan_3p0`/`wan_3p0_prime` (video), `meshy_v6`/`rodin_2p5_fast` (3D).
- **Verification harnesses**: `tools/artcraft_e2e_matrix.py` (budget-guarded 99-tool matrix), `tools/artcraft_drift_check.py` (static upstream route drift checker), `tools/artcraft_healthcheck.py`, the `tools/artcraft_*_evidence.py` collectors, and `docs/verification-evidence-report.md`.
- **CLI flags**: `--transport`, `--bind`, `--tool-mode`, `--confirm-over-credits`, `--auto-approve-under`, `--no-confirm`, `--allow-remote`.
- Zero-cost local conformance fixture tools and resources required by the official suite (image/audio/embedded/mixed content, progress, sampling, and elicitation including SEP-1034 defaults and SEP-1330 enum variants).

### Changed
- **Single binary**: `artcraft-mcp-server.exe` and `artcraft-mcp.exe` consolidated into `bin/artcraft-mcp.exe` v2.0.0; the `artcraft_mcp_server` crate is retired.
- **Auth rework**: `ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN` and `ARTCRAFT_API_KEY` are sent upstream as `Authorization: Bearer <secret>` and strictly win; `ARTCRAFT_SESSION`/`ARTCRAFT_AVT`, credential files, and the desktop cookie store remain the cookie fallback. HTTP transport access is gated by `ARTCRAFT_MCP_BEARER_TOKEN`.
- **Standalone crate**: upstream `artcraft_*` crates are pinned git dependencies (rev `9a1cc6b`); no monorepo checkout is needed to build.
- `tools/list` is canonically ordered and carries SEP-2549 caching hints.
- Tripo3D H3.1 now uses the upstream id `tripo3d_h3p1`; `tripo_3d_h3p1`, `tripo_h3_1`, and `tripo` remain accepted aliases.
- The `artcraft_generate_3d_object` input schema now lists the parameters the handler reads (`reference_image_media_tokens`, `enable_pbr`, `enable_texture`, `texture_quality`, `geometry_quality`) and drops the unused `quality`.
- `artcraft_estimate_cost` now prices all five generation media types — `image`, `video`, `audio`, `mesh`, and `splat` — routing audio and mesh through the upstream OmniGen cost endpoints.
- `schemas/*.json` regenerated from the live `tools/list` (99 files).
- `ARTCRAFT_MCP_STATE_SECRET` configures the `requestState` signing key for multi-instance HTTP deployments (random per-process otherwise).
- README and AGENTS.md rewritten for v2.0; the packaged Windows binary was rebuilt from the release crate (SHA-256 recorded in the evidence report).

### Removed
- **Voice conversion / voice designer tool family** — upstream retired the endpoints; removed rather than left as dead stubs.
- **`artcraft_studio_gen2` generation tool** — the upstream route never existed (hallucinated); removed.
- `artcraft_tts_generate` is deprecated: upstream `/v1/tts/inference` only returns a synthetic nonfunctional job token, so the tool explains the situation instead of enqueueing fake work (zero cost).

### Fixed
- Route drift across the catalog, each repair verified against upstream Actix registrations: `rate_media` singular path, `set_tags` path-parameter shape, bookmark list/create routes, comment list/create routes, character singular create + session list, TTS `/v1/tts/inference`, billing-portal redirect mapping, per-model generation endpoints for image edit/inpaint/splat/video edit, weight-search parameter shape, audio-upload accepted formats, `get_session_info` on `/v1/session` (replacing the removed `/v1/users/session_info`), and a real `terminate_job` implementation.
- The MCP conformance expected-failure baseline uses the `server:` array format and shrank from 22 expected failures to an empty baseline.
- `tools/artcraft_healthcheck.py` now fails on legacy initialize wire shapes and tool-level `isError` responses instead of reporting them as passes.
- Skill references listed model ids that do not exist upstream (`flux_3_pro`, `seedream_5_pro`, `seedream_5_ultra`, `seedance_2p5_fast`, `seedance_2p5_bp`, `tripo_h3_1`), the never-launched `seedance_2p0_u` variants, and 3D/splat parameter names the server does not accept.
- CI smoke-tests the checked-in Windows executable and runs the official conformance suite against the release build.

### Security
- **MRTR `requestState`** is HMAC-sealed and tamper-evident — a one-character modification is rejected with `tampered or unknown request state`; states expire after 15 minutes; one-shot idempotency keys prevent a retried approval from spending credits twice.
- **HTTP transport hardening**: `Origin` allowlist (loopback/`null`/absent) and a loopback `Host` allowlist — a hostile `Host: evil.example` receives 403.
- **Remote bind safety**: non-loopback binds are refused without `--allow-remote` and additionally require `ARTCRAFT_MCP_BEARER_TOKEN`; host `file_path` arguments are blocked for remote-served sessions unless `ARTCRAFT_MCP_ALLOW_HOST_PATHS=1`.
- **Upload integrity**: declared-vs-received byte counts are re-verified before a files-extension retry executes; zero-byte and oversized uploads are rejected; upload tokens and task handles are unguessable UUIDs.

## [1.1.0] - 2026-10-07

### Added
- **Full 61-Tool Engine Expansion**:
  - `artcraft_generate_audio`: Native audio and music generation powered by Suno.
  - `artcraft_download_media_file`: Direct downloading of generated or hosted media assets to local filesystem.
  - `artcraft_estimate_splat_cost`: Pre-generation exact credit estimation for WorldLabs Marble Gaussian splats.
  - `artcraft_create_checkout_session`: Instant Stripe credit package top-up checkout session generator.
  - `artcraft_create_subscription_checkout`: Direct Stripe subscription checkout session setup.
  - `artcraft_get_billing_portal_url`: Stripe customer billing and subscription portal access.
- **Enhanced 3D Mesh & Radiance Splatting**:
  - Added Tripo3D H3.1 (`tripo_h3_1`) model support to `artcraft_generate_3d_object`.
  - Added multi-view camera references (`back_media_file_token`, `left_media_file_token`, `right_media_file_token`).
  - Added retopology and PBR material synthesis controls (`enable_pbr`, `enable_texture`, `texture_quality`, `face_count`).
  - Added WorldLabs Marble 1.0 & 1.1 support with 360° panoramic mode (`is_panoramic`).
- **Extended Model Coverage**:
  - Video: MiniMax H3 / Turbo / Ultra, Seedance 2.5 / Fast / Ultra, Wan 3.0 / Prime, Kling 3.0 Pro, Sora 2, Beeble SwitchX.
  - Image: Flux 3 / Pro, Seedream 5 Pro / Ultra, GPT Image 2.5 (Flare / Sunburst), Midjourney 8.
- **Updated Schemas & Tool Definitions**:
  - 61 complete JSON-RPC tool schemas in `schemas/`.
  - Updated AI agent guidance in `skills/artcraft-mcp/SKILL.md` and reference tables.

---

## [1.0.0] - 2026-10-06

### Added
- **Native Rust MCP Server (`artcraft-mcp-server`)**:
  - Implements Model Context Protocol `2024-11-05` over stdio JSON-RPC.
  - Exposes 40+ native ArtCraft tools for Video, Image, 3D Mesh, Gaussian Splatting, TTS, and Audio generation.
  - Supports video models: Kling (3.0/2.6/2.1), Veo (3/3.1), Seedance (2.0/1.5), Sora 2, Grok.
  - Supports image models: Flux 1 Dev/Schnell, Flux Pro 1.1 Ultra, Nano Banana Pro, Seedream 4/5, Midjourney 7/8.
  - Supports 3D generation: Hunyuan 3D (2.0/2.1) and WorldLabs Marble Splatting.
- **5-Tool Cross-Platform CLI Suite (`mcp-server-package/tools/`)**:
  - `artcraft_runner.py`: Batch Video & Image automator with async polling.
  - `artcraft_3d_runner.py`: Hunyuan 3D mesh & Gaussian Splat generator.
  - `artcraft_angle_matrix.py`: 4-quadrant spatial camera orbit pass generator.
  - `artcraft_audio_weaver.py`: Audio prompt & TTS voice conversion weaver.
  - `artcraft_cost_estimator.py`: Live credit balance inspector & cost calculator.
- **Client Installer (`setup_mcp.py`)**:
  - 1-Click installer for Windsurf, Claude Desktop, and Cursor.
- **Agent Skill (`open-source-contributor`)**:
  - Encapsulates Open Source Guide best practices, PR workflows, and security checklists.
- **Documentation & Community Guidelines**:
  - Added `TOOLS.md`, `ArtCraft-MCP-Setup-Guide.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, and `SECURITY.md`.
