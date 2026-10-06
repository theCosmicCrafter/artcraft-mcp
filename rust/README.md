# ArtCraft MCP Server — Rust Source Implementation

This directory contains the Rust source code for the full Model Context Protocol (MCP) server implementation from the ArtCraft codebase.

## The Full 55-Tool MCP Server (`artcraft_mcp_cli/`)

The comprehensive implementation exposing ArtCraft's complete tool surface (55 distinct tools):
- **Generation**: Text-to-Image, Reference-to-Image, Video, 3D Object (Hunyuan 3D), Gaussian Splat (WorldLabs Marble).
- **Editing**: Inpainting, background removal, angle matrix camera orbits (`flux_2_lora_angles`), video editing.
- **Audio & Voice**: Text-to-Speech (TTS), voice cloning, custom dataset creation, audio upload.
- **Characters & Prompts**: Character creation, prompt management, tagging.
- **Social & Community**: Bookmarks, comments, ratings, referrals.
- **Account & Billing**: Credit inspection, subscription status, dynamic cost estimation.
- **Protocol**: Standard MCP JSON-RPC over stdio.

## Precompiled Binary

The standalone 64-bit Windows executable is ready to use in the `bin/` directory:
- `bin/artcraft-mcp.exe` (~12.1 MB): Statically compiled standalone binary with all 55 tools.

## Building from Monorepo

Because this crate shares typed tokens (`MediaFileToken`, `InferenceJobToken`) and data definitions with the ArtCraft engine, building from source is done within the ArtCraft monorepo:

```bash
cargo build --release -p artcraft-mcp
```
