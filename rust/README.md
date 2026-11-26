# ArtCraft MCP Server — Rust Source Implementation

This directory contains the Rust source code for the full Model Context Protocol (MCP) server implementation from the ArtCraft codebase.

## The Full 99-Tool MCP Server (`artcraft_mcp_cli/`)

The comprehensive implementation exposing ArtCraft's complete tool surface (99 distinct tools):
- **Generation**: Text-to-Image, Reference-to-Image, Video, 3D Object (Hunyuan 3D), Gaussian Splat (WorldLabs Marble).
- **Editing**: Inpainting, background removal, angle matrix camera orbits (`flux_2_lora_angles`), video editing.
- **Audio**: Suno audio generation, audio upload, TTS model search (3,768 models; legacy TTS inference is deprecated upstream).
- **Characters & Prompts**: Character creation, prompt management, tagging.
- **Organization**: Full folder system (create/rename/star/color/cover, subfolders, bulk media moves), API keys, MCP sessions.
- **Social & Community**: Bookmarks, comments, ratings, referrals.
- **Account & Billing**: Credit inspection, subscription status, dynamic cost estimation.
- **Protocol**: MCP JSON-RPC over stdio and HTTP — spec 2026-07-28 with progressive tool discovery, MRTR cost confirmation, tasks/files/apps extensions; legacy 2024-11-05 fully supported.

## Precompiled Binary

The standalone 64-bit Windows executable is ready to use in the `bin/` directory:
- `bin/artcraft-mcp.exe` (~12.1 MB): Statically compiled standalone binary with all 99 tools.

## Building from Monorepo

Because this crate shares typed tokens (`MediaFileToken`, `InferenceJobToken`) and data definitions with the ArtCraft engine, building from source is done within the ArtCraft monorepo:

```bash
cargo build --release -p artcraft-mcp
```
