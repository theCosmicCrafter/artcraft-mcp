# ArtCraft MCP Server

## Overview

The ArtCraft MCP server is a first-class Rust workspace crate at
`crates/tools/servers/artcraft_mcp_server`. It speaks the Model Context
Protocol (MCP) over stdio and uses `artcraft_client` to call the ArtCraft
backend.

## Architecture

```
artcraft_mcp_server
├── main.rs          # JSON-RPC stdio loop and request dispatch
├── mcp_protocol.rs  # Shared MCP types (requests, responses, tools)
├── handlers.rs      # Tool-call implementations
├── credentials.rs   # Credential resolution and mcp_sessions token exchange
└── tests/           # Unit tests for protocol parsing
```

## Authentication

The server authenticates with the ArtCraft backend using an
`mcp_session_private` token when available, falling back to API key or session
cookies. The `artcraft_client` `StorytellerCredentialSet` now supports an
`Authorization: Bearer <token>` header, which is added to the basic request
helpers.

## Companion bridge

`companion/backend/routes/mcp.py` provides FastAPI endpoints for the desktop
app to start, stop, and interrogate the Rust binary, and to exchange an API
key for an MCP session token.

## Tool list

See `crates/tools/servers/artcraft_mcp_server/README.md`.
