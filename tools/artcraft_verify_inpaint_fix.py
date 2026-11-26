#!/usr/bin/env python3
"""Verify the fixed artcraft_inpaint_image (flux_pro_1 default) end-to-end
through the MCP tool itself. Downloads the artifact as 13_inpaint_fixed_test.
"""
import json
import re
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from artcraft_e2e_matrix import Mcp, JOB_RE, redact  # noqa: E402

VERIF = Path(r"C:\Users\richk\Downloads\artcraft-mcp-verification")
IMG = "m_cefctfqs332dwfb04xpc066dqfb5yj"  # retained 512x512 red cube
MASK_FILE = r"C:\Windows\Temp\mask512.png"  # 512x512, matching dims


def credits(mcp):
    err, text, _ = mcp.tool("artcraft_get_credits", {})
    m = re.search(r"Banked credits: (\d+)", text)
    return int(m.group(1)) if m else None


def poll(mcp, job_token, timeout_s=300):
    deadline = time.time() + timeout_s
    last = ""
    while time.time() < deadline:
        err, text, _ = mcp.tool("artcraft_get_job_status", {"job_token": job_token}, timeout=30)
        if err:
            return None, f"poll error: {text}"
        last = text
        status = ""
        for line in text.splitlines():
            if line.lower().startswith("status:"):
                status = line.split(":", 1)[1].strip().lower()
        m_media = re.search(r"\b(m_[A-Za-z0-9]{6,})\b", text)
        if m_media or any(w in status for w in ("succeed", "complete", "finished", "success")):
            return text, None
        if any(w in status for w in ("failed", "error", "cancelled", "canceled")):
            return text, "job failed upstream"
        time.sleep(6)
    return last, "timeout"


def main():
    log = {}
    mcp = Mcp()
    mask_token = None
    try:
        start = credits(mcp)
        print(f"start credits: {start}")
        log["start_credits"] = start

        # schema sanity: tool exposes model param
        msg = mcp._request("tools/list", {})
        names = {t["name"]: t for t in msg["result"]["tools"]}
        inp = names.get("artcraft_inpaint_image", {})
        print(f"[schema] description: {inp.get('description','')[:150]}")
        log["description"] = inp.get("description")

        # upload 512x512 mask (free)
        err, text, _ = mcp.tool("artcraft_upload_image", {"file_path": MASK_FILE}, timeout=120)
        m_mask = re.search(r"\b((?:mf|m)_[A-Za-z0-9]{6,})\b", text)
        mask_token = m_mask.group(1) if (not err and m_mask) else None
        print(f"[mask] token={mask_token} err={err}")
        log["mask_token"] = mask_token
        if not mask_token:
            return 1

        # === FIXED PATH: default (flux_pro_1) through the MCP tool ===
        before = credits(mcp)
        err, text, _ = mcp.tool("artcraft_inpaint_image", {
            "prompt": "a bright green sphere",
            "image_media_token": IMG,
            "mask_media_token": mask_token,
        }, timeout=120)
        print(f"[inpaint default] err={err} :: {redact(text)[:220]}")
        log["default_call"] = redact(text)[:400]
        job = JOB_RE.search(text)
        if not err and job:
            final, perr = poll(mcp, job.group(1), timeout_s=300)
            print(f"[inpaint default] poll: {perr or 'done'} :: {redact(final or '')[:240]}")
            log["default_final"] = redact(final or "")[:400]
            m_media = re.search(r"\b(m_[A-Za-z0-9]{6,})\b", final or "")
            if m_media:
                out_tok = m_media.group(1)
                log["result_token"] = out_tok
                err2, text2, _ = mcp.tool("artcraft_get_media_file", {"media_token": out_tok})
                ext_m = re.search(r"Type:\s*(\w+)", text2)
                ext = "." + ext_m.group(1) if ext_m else ".png"
                fname = f"13_inpaint_fixed_test{ext}"
                err3, text3, _ = mcp.tool("artcraft_download_media_file",
                                          {"media_token": out_tok,
                                           "download_dir": str(VERIF), "filename": fname},
                                          timeout=180)
                print(f"[download] {fname}: err={err3} :: {redact(text3)[:140]}")
                log["artifact"] = str(VERIF / fname)
        after = credits(mcp)
        log["default_credits"] = (before - after) if (before is not None and after is not None) else None

        # === dispatch sanity: explicit flux_pro_1 alias ===
        # (skip second paid-gen if default already proved out; verify unknown-model error instead)
        err, text, _ = mcp.tool("artcraft_inpaint_image", {
            "model": "no_such_model",
            "prompt": "x",
            "image_media_token": IMG,
            "mask_media_token": mask_token,
        }, timeout=30)
        print(f"[unknown model] err={err} :: {redact(text)[:180]}")
        log["unknown_model"] = redact(text)[:300]

        # === dispatch sanity: juggernaut path still dispatches (expected upstream 500) ===
        err, text, _ = mcp.tool("artcraft_inpaint_image", {
            "model": "flux_dev_juggernaut",
            "prompt": "a green sphere",
            "image_media_token": IMG,
            "mask_media_token": mask_token,
        }, timeout=90)
        print(f"[juggernaut] err={err} :: {redact(text)[:200]}")
        log["juggernaut"] = redact(text)[:300]

        end = credits(mcp)
        log["end_credits"] = end
        print(f"\nend credits: {end}  delta: {(start - end) if (start and end) else '?'}")
    finally:
        if mask_token:
            err, text, _ = mcp.tool("artcraft_delete_media_file", {"media_token": mask_token}, timeout=60)
            print(f"cleanup mask: {'ok' if not err else 'err'}")
        mcp.close()
    Path(ROOT / "tools" / "verify_inpaint_fix_log.json").write_text(json.dumps(log, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
