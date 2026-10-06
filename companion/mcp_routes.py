"""MCP server lifecycle routes for the ArtCraft desktop companion.

These endpoints let the desktop UI start, stop, and interrogate the bundled
`artcraft-mcp-server` Rust binary, and exchange an ArtCraft API key or session
for an MCP session token.
"""

import asyncio
import os
import platform
import shlex
import sys
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from core.logging import get_logger
from config import get_settings

log = get_logger(__name__)

router = APIRouter(prefix="/api/mcp", tags=["mcp"])

# Global process handle for the spawned MCP server.
# In a future iteration this may move to a proper process manager / actor.
_mcp_server_process: Optional[asyncio.subprocess.Process] = None


class McpTokenRequest(BaseModel):
    """Request an MCP session token from the ArtCraft backend."""
    api_key: Optional[str] = None
    session_cookie: Optional[str] = None
    avt_cookie: Optional[str] = None
    api_host: str = "https://api.storyteller.ai"


class McpTokenResponse(BaseModel):
    success: bool
    private_session_token: str
    private_refresh_token: str


def _find_default_mcp_binary() -> Optional[Path]:
    """Locate the bundled artcraft-mcp-server binary relative to this file."""
    # companion/backend/routes/mcp.py -> repo root
    repo_root = Path(__file__).resolve().parents[3]
    binary_name = "artcraft-mcp-server.exe" if platform.system() == "Windows" else "artcraft-mcp-server"

    for profile in ("release", "debug"):
        candidate = repo_root / "target" / profile / binary_name
        if candidate.exists():
            return candidate

    return None


def _resolve_mcp_binary(override: Optional[str] = None) -> str:
    """Resolve the server binary path from override, env, or default search."""
    if override:
        candidate = Path(override).resolve()
        if not candidate.is_file():
            raise FileNotFoundError(f"MCP server binary not found: {override}")
        allowed_names = {"artcraft-mcp-server.exe", "artcraft-mcp-server", "artcraft-mcp.exe", "artcraft-mcp"}
        if candidate.name.lower() not in allowed_names:
            raise ValueError(f"Invalid MCP binary name: {candidate.name}. Allowed: {allowed_names}")
        return str(candidate)

    env_path = os.environ.get("ARTCRAFT_MCP_SERVER_PATH")
    if env_path:
        if not os.path.isfile(env_path):
            raise FileNotFoundError(f"MCP server binary not found: {env_path}")
        return env_path

    default = _find_default_mcp_binary()
    if default:
        return str(default)

    raise FileNotFoundError(
        "Could not find artcraft-mcp-server binary. Set ARTCRAFT_MCP_SERVER_PATH or build it."
    )


@router.get("/health")
async def mcp_health() -> dict:
    """Return whether the MCP server process is currently running."""
    if _mcp_server_process is None:
        return {"running": False}

    if _mcp_server_process.returncode is None:
        return {
            "running": True,
            "pid": _mcp_server_process.pid,
            "returncode": None,
        }

    return {
        "running": False,
        "pid": _mcp_server_process.pid,
        "returncode": _mcp_server_process.returncode,
    }


@router.post("/start")
async def mcp_start(override_path: Optional[str] = None) -> dict:
    """Start the artcraft-mcp-server subprocess."""
    global _mcp_server_process

    if _mcp_server_process is not None and _mcp_server_process.returncode is None:
        return {
            "started": False,
            "reason": "MCP server already running",
            "pid": _mcp_server_process.pid,
        }

    try:
        binary = _resolve_mcp_binary(override_path)
    except FileNotFoundError as exc:
        raise HTTPException(status_code=503, detail=str(exc)) from exc

    # Use asyncio subprocess so we don't block the event loop.
    # stdin/stdout are left connected for stdio transport.
    try:
        _mcp_server_process = await asyncio.create_subprocess_exec(
            binary,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
    except OSError as exc:
        log.error("failed to start mcp server", binary=binary, error=str(exc))
        raise HTTPException(status_code=500, detail=f"Failed to start MCP server: {exc}") from exc

    log.info("mcp server started", binary=binary, pid=_mcp_server_process.pid)
    return {
        "started": True,
        "pid": _mcp_server_process.pid,
        "binary": binary,
    }


@router.post("/stop")
async def mcp_stop() -> dict:
    """Stop the artcraft-mcp-server subprocess if it is running."""
    global _mcp_server_process

    if _mcp_server_process is None or _mcp_server_process.returncode is not None:
        return {"stopped": False, "reason": "MCP server not running"}

    try:
        _mcp_server_process.terminate()
        try:
            await asyncio.wait_for(_mcp_server_process.wait(), timeout=5.0)
        except asyncio.TimeoutError:
            _mcp_server_process.kill()
            await _mcp_server_process.wait()
    except ProcessLookupError:
        pass

    log.info("mcp server stopped", pid=_mcp_server_process.pid)
    return {"stopped": True, "pid": _mcp_server_process.pid}


@router.post("/token")
async def mcp_token(request: McpTokenRequest) -> McpTokenResponse:
    """Exchange an ArtCraft API key or session cookies for an MCP session token."""
    import aiohttp

    if not request.api_key and not request.session_cookie:
        raise HTTPException(
            status_code=400,
            detail="Either api_key or session_cookie must be provided",
        )

    body = {
        "maybe_mcp_client_name": "artcraft-mcp-server",
        "maybe_mcp_client_version": "0.1.0",
    }

    headers: dict[str, str] = {}
    if request.api_key:
        headers["Authorization"] = f"Bearer {request.api_key}"
    else:
        cookies = []
        if request.session_cookie:
            cookies.append(f"session={request.session_cookie}")
        if request.avt_cookie:
            cookies.append(f"visitor={request.avt_cookie}")
        headers["Cookie"] = "; ".join(cookies)

    async with aiohttp.ClientSession() as session:
        async with session.post(
            f"{request.api_host}/v1/mcp/session/create",
            json=body,
            headers=headers,
            timeout=aiohttp.ClientTimeout(total=30),
        ) as resp:
            if resp.status == 401:
                raise HTTPException(status_code=401, detail="Invalid credentials")
            if resp.status >= 400:
                text = await resp.text()
                raise HTTPException(status_code=resp.status, detail=text)

            data = await resp.json()

    return McpTokenResponse(
        success=data.get("success", True),
        private_session_token=data["private_session_token"],
        private_refresh_token=data["private_refresh_token"],
    )
