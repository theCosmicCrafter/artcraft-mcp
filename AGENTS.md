# AGENTS.md — ArtCraft MCP Project

Standalone project bundling the ArtCraft Model Context Protocol (MCP) server, its precompiled binary, automation CLI tools, agent skills, JSON schemas, verification harnesses, and companion bridge routes.

## Quick Facts
- **Binary**: `.\bin\artcraft-mcp.exe` — the single v2.0.0 server. There is no `artcraft-mcp-server.exe` anymore; the two-binary layout was consolidated in v2.0.
  - 99 business tools (legacy `2024-11-05` clients see the full list; `2026-07-28` clients see a progressive catalog — hot tools plus search/describe/call-deferred meta-tools, 10 total when the client declares tasks support)
  - Dual transport: stdio (default) and Streamable HTTP (`--transport http --bind 127.0.0.1:8787`, serves `/mcp` + `/health`)
  - MRTR cost-confirmation gate, tasks/files/apps extensions — see `README.md` and `docs/verification-evidence-report.md`
- **CLI Suite**: `.\tools\*.py` (requires Python 3.10+, `$env:PYTHONUTF8="1"`)
- **Agent Skill**: `.\skills\artcraft-mcp\SKILL.md`
- **Schemas**: `.\schemas\*.json` (99 tool definitions, synced from live `tools/list`)
- **Rust Source**: `.\rust\artcraft_mcp_cli\` — one standalone crate (`artcraft-mcp`); upstream crates are pinned git dependencies, so no monorepo checkout is needed. `artcraft_mcp_server` no longer exists.

## Machine & Workspace Rules
- Always set `PYTHONUTF8=1` on every Python invocation.
- Do NOT run heavy builds or long GPU jobs without checking `/queue` or user permission.
- `tools\artcraft_e2e_matrix.py --phases all` spends real ArtCraft credits. Use `--phases free` for zero-cost validation and always pass `--budget-credits` with `--phases all`. Legacy sessions execute billable tools directly (MRTR only gates modern elicitation-capable clients), so spend is real either way.
- Credential hierarchy automatically resolves:
  1. `ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN`
  2. `ARTCRAFT_API_KEY`
  3. `ARTCRAFT_SESSION` & `ARTCRAFT_AVT`
  4. Local files in `~\Artcraft\credentials\`
  5. Local desktop cookie store in `%LOCALAPPDATA%\ai.artcraft.app\.cookies`
- Items 1–2 are sent upstream as `Authorization: Bearer <secret>` and strictly win; the cookie pair is the legacy fallback.

## Common Verification Commands
```powershell
# Verify binary responds to JSON-RPC over stdio (legacy clients see all 99 tools)
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | .\bin\artcraft-mcp.exe

# Full diagnostic smoke (binary discovery, protocol, auth, cost estimate)
$env:PYTHONUTF8="1"; python tools\artcraft_healthcheck.py

# Check credits and live balance
$env:PYTHONUTF8="1"; python tools\artcraft_cost_estimator.py --check-balance

# Zero-cost end-to-end matrix across all 99 tools
$env:PYTHONUTF8="1"; python tools\artcraft_e2e_matrix.py --phases free

# Budgeted full matrix — spends real credits, capped
$env:PYTHONUTF8="1"; python tools\artcraft_e2e_matrix.py --phases all --budget-credits 150 --report tools\e2e_matrix_report.json

# 1-Click setup across installed MCP clients
$env:PYTHONUTF8="1"; python setup_mcp.py
```

## Rust Gates
```powershell
cd rust\artcraft_mcp_cli
cargo check --all-targets
cargo test --all-targets        # unit + protocol integration tests
cargo clippy --all-targets -- -D warnings
cargo build --release           # then copy target\release\artcraft-mcp.exe over bin\artcraft-mcp.exe
```

## MCP Conformance Suite
```powershell
.\bin\artcraft-mcp.exe --transport http --bind 127.0.0.1:8787
npx -y @modelcontextprotocol/conformance server --url http://127.0.0.1:8787/mcp --suite active --expected-failures conformance\conformance-baseline.yml
```
- Current result: 40/40 checks across 30/30 scenarios; `conformance\conformance-baseline.yml` is `server: []` (no expected failures). Keep it empty — stale entries fail the run.

## Upstream Route Drift Check
- Run `$env:PYTHONUTF8="1"; python tools\artcraft_drift_check.py` after syncing the local `artcraft-services` clone; pass `--upstream <checkout>` when it is not at the default location.
- The checker statically diffs MCP tool endpoint usage against upstream Actix registrations, regenerates `tools\upstream_route_table.json`, and writes an optional machine-readable report with `--json <path>`.
- Exit code `1` means an MCP-called route is missing upstream or uses an unregistered verb; uncovered upstream routes are informational coverage gaps, not failures. Use `--no-write-route-table` for read-only report generation.
- Current watermark: 94 usages matched, 0 missing, 0 verb mismatches against 364 upstream routes.

## Known upstream limitations — do not "fix" locally
- `artcraft_inpaint_image`, `artcraft_generate_splat`, `artcraft_edit_video`: reproducible upstream HTTP 500s with valid inputs; zero credits lost. Evidence: `docs/verification-evidence-report.md`.
- Voice conversion/designer family: **removed** — upstream retired the endpoints. `artcraft_tts_generate` is deprecated (upstream returns only a synthetic nonfunctional job token).
- `artcraft_generate_audio` (Suno): observed upstream job stall — enqueue charged 3 credits, job stayed `pending` 0% for 600+ s.
- `artcraft_estimate_cost` prices all five generation media types (image, video, audio, mesh, splat) via the OmniGen cost routes.
- `tasks/cancel` stops local tracking only; the upstream API has no job-cancel endpoint.
