# ArtCraft MCP Server

A Model Context Protocol (MCP) server that exposes ArtCraft generation, media,
and account tools to MCP clients such as Claude Desktop, Cursor, and other
IDEs.

## Building

From the repository root:

```bash
cargo build -p artcraft-mcp-server
```

For release:

```bash
cargo build -p artcraft-mcp-server --release
```

## Authentication

The server tries the following credential sources, in order:

1. `ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN` environment variable.
2. `ARTCRAFT_API_KEY` environment variable (exchanged for an MCP session).
3. `ARTCRAFT_SESSION` and `ARTCRAFT_AVT` cookies from environment variables.
4. Standard ArtCraft desktop credential files in `~/.config/artcraft/` or
   `~/Artcraft/credentials/`.
5. `artcraft_cookies.txt` in the current working directory.

To mint an MCP session token from an API key, call
`POST /v1/mcp/session/create` on the ArtCraft backend.

## Running

### stdio (default)

```bash
artcraft-mcp-server
```

MCP clients launch the binary and communicate over stdin/stdout.

### Example `claude_desktop_config.json`

```json
{
  "mcpServers": {
    "artcraft": {
      "command": "/path/to/artcraft-mcp-server",
      "env": {
        "ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN": "mcp_session_private_...",
        "ARTCRAFT_ENVIRONMENT": "prod"
      }
    }
  }
}
```

Replace the token with one obtained from `POST /v1/mcp/session/create`.

## Tools

- `initialize`, `tools/list`, `tools/call` (MCP protocol)
- `generate_image`, `generate_video`
- `list_jobs`, `get_job_status`
- `upload_media`, `get_media_file`, `download_media_file`, `delete_media_file`
- `get_credits`, `get_subscription`, `create_checkout_session`, `get_billing_portal_url`
- `estimate_image_cost`, `estimate_video_cost`
- `create_prompt`, `list_models`, `check_provider_credentials`

## Companion launch bridge

The ArtCraft desktop companion exposes FastAPI routes under `/api/mcp`:

- `GET /api/mcp/health`
- `POST /api/mcp/start`
- `POST /api/mcp/stop`
- `POST /api/mcp/token`

These let the desktop app spawn the Rust binary and exchange user credentials
for an MCP session token.

## Testing

```bash
cargo test -p artcraft-mcp-server
```
