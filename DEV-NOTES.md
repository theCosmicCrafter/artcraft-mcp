# ArtCraft MCP Server - Development Package

## Package Contents

This package contains the ArtCraft MCP Server ready for development and testing.

### Files Included:
- `artcraft-mcp.exe` - The full 55-tool MCP server executable (Rust, release build)
- `ArtCraft-MCP-Setup-Guide.md` - Complete setup and configuration guide
- `README.md` - ArtCraft MCP project README
- `LICENSE.md` - License information
- `skills/` - Agent skill files for AI assistants
  - `SKILL.md` - Main skill documentation
  - `references/` - Model and configuration references
    - `aspect-ratios.md`
    - `image-models.md`
    - `video-models.md`
    - `three_d_and_splat_models.md`

## Quick Start for Dev Team

### 1. Place the Executable
Copy `artcraft-mcp.exe` to any location on your system (e.g., `C:\Tools\artcraft-mcp.exe` or use `bin/artcraft-mcp.exe`)

### 2. Set Up Credentials
Create credential files at `C:\Users\YourUsername\Artcraft\credentials\`:
- `artcraft_session.txt` - Your ArtCraft session cookie
- `artcraft_avt.txt` - Your ArtCraft visitor cookie

Or install the ArtCraft Desktop App and log in (credentials auto-detected).

### 3. Configure MCP Client
Add to your MCP config (e.g., `~/.windsurf/mcp_config.json`):
```json
{
  "mcpServers": {
    "artcraft": {
      "command": "C:\\path\\to\\artcraft-mcp.exe",
      "args": [],
      "description": "ArtCraft AI generation platform (Full 55-Tool Engine)",
      "enabled": true
    }
  }
}
```

### 4. Install Skills
Copy the `skills/` folder to your skills directory for AI assistant guidance.

## Technical Details

- **Language**: Rust
- **Build**: Release mode (optimized)
- **Version**: 0.1.0
- **Protocol**: Model Context Protocol (MCP) over stdio
- **Tools**: 55 tools for image/video generation, 3D, voice, TTS, characters, LoRAs, and community features

## Known Issues

- Requires active ArtCraft account and credentials

## Development Notes

Source code is in the main ArtCraft repository root.

To rebuild from source:
```bash
cd <path-to-artcraft-repo>
cargo build --release -p artcraft-mcp
```

## Support

See `ArtCraft-MCP-Setup-Guide.md` for detailed troubleshooting and configuration options.
