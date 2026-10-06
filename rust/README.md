# ArtCraft MCP Server — Rust Source Implementations

This directory contains the Rust source code for the ArtCraft Model Context Protocol (MCP) server implementations from the ArtCraft codebase.

## Implementations Included

### 1. `artcraft_mcp_server/` (Official Core Server)
The modern, streamlined implementation integrating with the ArtCraft backend:
- **Protocol**: MCP `2024-11-05` over stdio JSON-RPC.
- **Authentication**: Multi-tier credentials hierarchy:
  1. `ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN` (mcp_sessions private token)
  2. `ARTCRAFT_API_KEY` (automatically exchanged for an MCP session token)
  3. `ARTCRAFT_SESSION` and `ARTCRAFT_AVT` environment variables
  4. Desktop credentials at `~/Artcraft/credentials/` or `~/.config/artcraft/`
  5. Tauri cookie store at `%LOCALAPPDATA%\ai.artcraft.app\.cookies`
- **Core Tools**:
  - `generate_image`: OmniGen image generation with Flux, GPT-Image, Grok, Midjourney, Nano Banana, Seedream.
  - `generate_video`: OmniGen video generation with Kling, Veo, Seedance, Sora 2, Grok.
  - `list_jobs`, `get_job_status`: Background inference job tracking.
  - `upload_media`, `get_media_file`, `download_media_file`, `delete_media_file`: Media asset pipeline.
  - `get_credits`, `get_subscription`, `create_checkout_session`, `get_billing_portal_url`: Billing and account management.
  - `estimate_image_cost`, `estimate_video_cost`: Pre-generation cost calculations.
  - `create_prompt`, `list_models`, `check_provider_credentials`: Prompt management and provider inspection.
- **Testing**: Includes unit tests in `src/tests/mcp_protocol_tests.rs`.

### 2. `artcraft_mcp_cli/` (Comprehensive 55-Tool Server)
The extended community/CLI edition exposing the full 55-tool surface:
- Exposes tools across all ArtCraft modules:
  - Generation: Image, Video, 3D Object (Hunyuan 3D), Gaussian Splat (WorldLabs Marble).
  - Editing: Inpainting, background removal, angle matrix camera orbits.
  - Audio & Voice: TTS generation, voice cloning, audio upload.
  - Characters & Prompts: Character creation, prompt management, tagging.
  - Social & Community: Bookmarks, comments, ratings, referrals.
- Direct JSON-RPC stdio event loop.

## Precompiled Binaries

Both implementations have prebuilt 64-bit Windows executables available in the `bin/` directory:
- `bin/artcraft-mcp-server.exe` (~11.8 MB): Built from `artcraft_mcp_server`.
- `bin/artcraft-mcp.exe` (~12.1 MB): Built from `artcraft_mcp_cli`.

## Building from Monorepo

Because these crates share typed tokens (`MediaFileToken`, `InferenceJobToken`) and data definitions with the ArtCraft engine, building from source is done within the ArtCraft monorepo:

```bash
# Build official core server:
cargo build --release -p artcraft-mcp-server

# Build comprehensive CLI server:
cargo build --release -p artcraft-mcp
```
