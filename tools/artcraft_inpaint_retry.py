#!/usr/bin/env python3
"""Retry flux_pro_1 inpaint with a dimension-matched 512x512 mask."""
import json
import re
import sys
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from artcraft_e2e_matrix import Mcp, JOB_RE, redact  # noqa: E402

import requests  # noqa: E402

API = "https://api.storyteller.ai"
IMG = "m_cefctfqs332dwfb04xpc066dqfb5yj"
MASK_FILE = r"C:\Windows\Temp\mask512.png"


def credits(mcp):
    err, text, _ = mcp.tool("artcraft_get_credits", {})
    m = re.search(r"Banked credits: (\d+)", text)
    return int(m.group(1)) if m else None


def load_cookie_header():
    home = Path.home() / "Artcraft" / "credentials"
    session = (home / "artcraft_session.txt").read_text().strip()
    avt = (home / "artcraft_avt.txt").read_text().strip()
    return f"visitor={avt}; session={session}"


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
    mcp = Mcp()
    log = {}
    try:
        start = credits(mcp)
        print(f"start credits: {start}")

        err, text, _ = mcp.tool("artcraft_upload_image", {"file_path": MASK_FILE}, timeout=120)
        print(f"mask upload: err={err} :: {redact(text)[:160]}")
        m_mask = re.search(r"\b((?:mf|m)_[A-Za-z0-9]{6,})\b", text)
        mask_token = m_mask.group(1) if (not err and m_mask) else None
        print(f"mask token: {mask_token}")
        log["mask_token"] = mask_token
        if not mask_token:
            return 1

        before = credits(mcp)
        r = requests.post(
            API + "/v1/generate/image/inpaint/flux_pro_1",
            json={
                "uuid_idempotency_token": str(uuid.uuid4()),
                "prompt": "a bright green sphere",
                "image_media_token": IMG,
                "mask_media_token": mask_token,
                "num_images": "one",
            },
            headers={"Content-Type": "application/json",
                     "Accept": "application/json",
                     "Cookie": load_cookie_header()},
            timeout=60,
        )
        print(f"POST: {r.status_code} :: {redact(r.text)[:220]}")
        log["http_status"] = r.status_code
        log["response"] = redact(r.text)[:400]
        if r.ok:
            job = r.json().get("inference_job_token")
            log["job"] = job
            final, perr = poll(mcp, job, timeout_s=300)
            print(f"poll: {perr or 'done'} :: {redact(final or '')[:240]}")
            log["final"] = redact(final or "")[:400]
            m_media = re.search(r"\b(m_[A-Za-z0-9]{6,})\b", final or "")
            if m_media:
                log["result_token"] = m_media.group(1)
        after = credits(mcp)
        log["credits"] = (before - after) if (before is not None and after is not None) else None
        print(f"credits delta: {log['credits']}")

        # cleanup mask
        err, text, _ = mcp.tool("artcraft_delete_media_file", {"media_token": mask_token}, timeout=60)
        print(f"cleanup mask: {'ok' if not err else redact(text)[:100]}")
    finally:
        mcp.close()
    Path(ROOT / "tools" / "inpaint_retry_log.json").write_text(json.dumps(log, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
