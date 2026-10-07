# AGENTS.md — ArtCraft MCP Project

Standalone project bundling all ArtCraft Model Context Protocol (MCP) server materials, precompiled binaries, automation CLI tools, agent skills, JSON schemas, and companion bridge routes.

## Quick Facts
- **Binary Locations**:
  - `.\bin\artcraft-mcp-server.exe` — Modern core server (MCP 2024-11-05, mcp_sessions auth)
  - `.\bin\artcraft-mcp.exe` — Comprehensive 61-tool community edition
- **CLI Suite**: `.\tools\*.py` (requires Python 3.10+, `$env:PYTHONUTF8="1"`)
- **Agent Skill**: `.\skills\artcraft-mcp\SKILL.md`
- **Schemas**: `.\schemas\*.json` (61 tool definitions)
- **Rust Source**: `.\rust\artcraft_mcp_server\` and `.\rust\artcraft_mcp_cli\`

## Machine & Workspace Rules
- Always set `PYTHONUTF8=1` on every Python invocation.
- Do NOT run heavy builds or long GPU jobs without checking `/queue` or user permission.
- Credential hierarchy automatically resolves:
  1. `ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN`
  2. `ARTCRAFT_API_KEY`
  3. `ARTCRAFT_SESSION` & `ARTCRAFT_AVT`
  4. Local files in `~\Artcraft\credentials\`
  5. Local desktop cookie store in `%LOCALAPPDATA%\ai.artcraft.app\.cookies`

## Common Verification Commands
```powershell
# Verify binary responds to JSON-RPC over stdio
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | .\bin\artcraft-mcp-server.exe

# Check credits and live balance
$env:PYTHONUTF8="1"; python tools\artcraft_cost_estimator.py --check-balance

# 1-Click setup across installed MCP clients
$env:PYTHONUTF8="1"; python setup_mcp.py
```
