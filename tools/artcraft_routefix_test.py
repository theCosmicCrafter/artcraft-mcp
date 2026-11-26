#!/usr/bin/env python3
"""One-off route-fix verification: live-test triposplat + flux_pro_1 inpaint.

Working-tree-only harness. Reuses the Mcp stdio session class from
artcraft_e2e_matrix.py. Credentials resolve the same way the MCP server
does (~/Artcraft/credentials files) for the one direct upstream POST the
MCP does not yet expose. Secrets are never printed.

Usage:
  python tools/artcraft_routefix_test.py
"""

import json
import os
import re
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from artcraft_e2e_matrix import Mcp, JOB_RE, redact  # noqa: E402

import requests  # noqa: E402

VERIF = Path(r"C:\Users\richk\Downloads\artcraft-mcp-verification")
FFMPEG = os.environ.get("FFMPEG", r"C:\ffmpeg-gpl\ffmpeg-master-latest-win64-gpl\bin\ffmpeg.exe")
API = "https://api.storyteller.ai"
RETAINED_IMG = "m_cefctfqs332dwfb04xpc066dqfb5yj"  # 01_flux_red_cube, retained on account


def credits(mcp):
    err, text, _ = mcp.tool("artcraft_get_credits", {})
    m = re.search(r"Banked credits: (\d+)", text)
    return int(m.group(1)) if m else None


def load_cookie_header():
    home = Path.home() / "Artcraft" / "credentials"
    session = (home / "artcraft_session.txt").read_text().strip()
    avt = (home / "artcraft_avt.txt").read_text().strip()
    return f"visitor={avt}; session={session}"


def poll(mcp, job_token, timeout_s=420):
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
        time.sleep(8)
    return last, "timeout"


def media_ext(mcp, token):
    """Return file extension for a media token ('.ply', '.png', ...)."""
    err, text, _ = mcp.tool("artcraft_get_media_file", {"media_token": token})
    if err:
        return ".bin"
    m = re.search(r"Type:\s*(\w+)", text)
    return "." + m.group(1) if m else ".bin"


def main():
    log = {}
    tmp = Path(tempfile.mkdtemp(prefix="routefix-"))
    mask_path = tmp / "mask.png"
    # black mask w/ white center square => real inpaint region
    subprocess.run([FFMPEG, "-y", "-f", "lavfi",
                    "-i", "color=c=black:s=256x256,drawbox=x=64:y=64:w=128:h=128:color=white:t=fill",
                    "-frames:v", "1", str(mask_path)], capture_output=True)

    mcp = Mcp()
    try:
        start = credits(mcp)
        print(f"start credits: {start}")
        log["start_credits"] = start

        # ---- zero-cost recon ----
        err, text, _ = mcp.tool("artcraft_estimate_splat_cost",
                                {"model": "triposplat", "has_reference_image": True})
        print(f"[est] splat triposplat: err={err} :: {redact(text)[:200]}")
        log["est_triposplat"] = text

        err, text, _ = mcp.tool("artcraft_estimate_cost",
                                {"media_type": "image", "model": "flux_pro_1p1",
                                 "generation_mode": "image_edit", "input_image_count": 2})
        print(f"[est] image flux_pro_1p1 image_edit(2): err={err} :: {redact(text)[:200]}")
        log["est_fluxpro_edit"] = text

        # ---- source image: reuse retained media (free) ----
        err, text, _ = mcp.tool("artcraft_get_media_file", {"media_token": RETAINED_IMG})
        print(f"[img] retained media check: err={err} :: {redact(text)[:160]}")
        img_token = RETAINED_IMG if not err else None

        if img_token is None:
            err, text, _ = mcp.tool("artcraft_generate_image", {
                "prompt": "a small red cube on a white background, simple test image",
                "model": "flux_1_schnell", "image_batch_count": 1}, timeout=180)
            j = JOB_RE.search(text)
            if not err and j:
                final, perr = poll(mcp, j.group(1), timeout_s=240)
                m_media = re.search(r"\b(m_[A-Za-z0-9]{6,})\b", final or "")
                img_token = m_media.group(1) if m_media else None
                print(f"[img] generated: {img_token} ({perr or 'ok'})")
        print(f"[img] using image token: {img_token}")
        log["image_token"] = img_token

        # ---- mask upload (free) ----
        mask_token = None
        if mask_path.exists():
            err, text, _ = mcp.tool("artcraft_upload_image", {"file_path": str(mask_path)}, timeout=180)
            m_mask = re.search(r"\b((?:mf|m)_[A-Za-z0-9]{6,})\b", text)
            if not err and m_mask:
                mask_token = m_mask.group(1)
            print(f"[mask] upload: err={err} token={mask_token} :: {redact(text)[:140]}")
        log["mask_token"] = mask_token

        # ===== TEST A: triposplat via MCP tool =====
        log["triposplat"] = {}
        if img_token:
            before = credits(mcp)
            err, text, _ = mcp.tool("artcraft_generate_splat",
                                    {"model": "triposplat", "image_media_token": img_token},
                                    timeout=120)
            print(f"[splat] enqueue: err={err} :: {redact(text)[:220]}")
            log["triposplat"]["enqueue"] = redact(text)[:400]
            j = JOB_RE.search(text)
            if not err and j:
                job = j.group(1)
                log["triposplat"]["job"] = job
                final, perr = poll(mcp, job, timeout_s=600)
                print(f"[splat] poll: {perr or 'completed'} :: {redact((final or ''))[:220]}")
                log["triposplat"]["final"] = redact(final or "")[:400]
                m_media = re.search(r"\b(m_[A-Za-z0-9]{6,})\b", final or "")
                if m_media:
                    out_tok = m_media.group(1)
                    log["triposplat"]["result_token"] = out_tok
                    ext = media_ext(mcp, out_tok)
                    fname = f"12_triposplat_test{ext}"
                    err2, text2, _ = mcp.tool("artcraft_download_media_file",
                                              {"media_token": out_tok,
                                               "download_dir": str(VERIF), "filename": fname},
                                              timeout=180)
                    print(f"[splat] download {fname}: err={err2} :: {redact(text2)[:140]}")
                    log["triposplat"]["artifact"] = str(VERIF / fname)
            after = credits(mcp)
            if before is not None and after is not None:
                log["triposplat"]["credits"] = before - after
                print(f"[splat] credits: {before - after}")
        else:
            print("[splat] SKIP: no image token")

        # ===== TEST B: flux_pro_1 inpaint, direct upstream POST =====
        log["flux_pro_1"] = {}
        if img_token and mask_token:
            before = credits(mcp)
            body = {
                "uuid_idempotency_token": str(uuid.uuid4()),
                "prompt": "a bright green sphere",
                "image_media_token": img_token,
                "mask_media_token": mask_token,
                "num_images": "one",
            }
            try:
                r = requests.post(
                    API + "/v1/generate/image/inpaint/flux_pro_1",
                    json=body,
                    headers={"Content-Type": "application/json",
                             "Accept": "application/json",
                             "Cookie": load_cookie_header()},
                    timeout=60,
                )
                print(f"[inpaint] POST status={r.status_code} :: {redact(r.text)[:220]}")
                log["flux_pro_1"]["http_status"] = r.status_code
                log["flux_pro_1"]["response"] = redact(r.text)[:400]
                if r.ok:
                    resp = r.json()
                    job = resp.get("inference_job_token")
                    log["flux_pro_1"]["job"] = job
                    if job:
                        final, perr = poll(mcp, job, timeout_s=300)
                        print(f"[inpaint] poll: {perr or 'completed'} :: {redact((final or ''))[:220]}")
                        log["flux_pro_1"]["final"] = redact(final or "")[:400]
                        m_media = re.search(r"\b(m_[A-Za-z0-9]{6,})\b", final or "")
                        if m_media:
                            log["flux_pro_1"]["result_token"] = m_media.group(1)
            except Exception as e:
                print(f"[inpaint] POST exception: {e}")
                log["flux_pro_1"]["exception"] = str(e)
            after = credits(mcp)
            if before is not None and after is not None:
                log["flux_pro_1"]["credits"] = before - after
                print(f"[inpaint] credits: {before - after}")
        else:
            print("[inpaint] SKIP: missing image or mask token")

        end = credits(mcp)
        log["end_credits"] = end
        print(f"\nend credits: {end}  total delta: {(start - end) if (start and end) else '?'}")

        # cleanup: mask upload only (image is retained/pre-existing; result media kept as artifacts)
        if mask_token:
            err, text, _ = mcp.tool("artcraft_delete_media_file", {"media_token": mask_token}, timeout=60)
            print(f"cleanup mask: {'ok' if not err else 'err ' + redact(text)[:100]}")
    finally:
        mcp.close()

    out = Path(__file__).resolve().parent / "routefix_test_log.json"
    out.write_text(json.dumps(log, indent=2), encoding="utf-8")
    print(f"log: {out}")


if __name__ == "__main__":
    sys.exit(main())
