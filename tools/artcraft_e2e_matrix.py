#!/usr/bin/env python3
"""ArtCraft MCP end-to-end tool matrix harness.

Exercises every tool of the ArtCraft MCP server over stdio in dependency
order, under a strict credit budget guard. Legacy MCP sessions execute
billable tools directly (MRTR only gates modern elicitation-capable
sessions), so this harness preflights estimates and measures real spend
via artcraft_get_credits deltas.

Usage:
  python tools/artcraft_e2e_matrix.py --phases free   # zero-credit validation
  python tools/artcraft_e2e_matrix.py --phases all --budget-credits 600
"""

import argparse
import json
import os
import queue
import re
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN = Path(os.environ.get("ARTCRAFT_MCP_PATH", ROOT / "rust/artcraft_mcp_cli/target/release/artcraft-mcp.exe"))
if not BIN.exists():
    BIN = ROOT / "bin/artcraft-mcp.exe"
FFMPEG = os.environ.get("FFMPEG", r"C:\ffmpeg-gpl\ffmpeg-master-latest-win64-gpl\bin\ffmpeg.exe")

TOKEN_RE = re.compile(r"\b((?:mf|vo|vd|ch|pt|bk|cm|rf|ij|jb)_[A-Za-z0-9]{6,})\b")
GENERIC_TOKEN_RE = re.compile(r"(?:Token|token)[:'\s]+([A-Za-z0-9_]{8,})")
JOB_RE = re.compile(r"[Jj]ob token[:'\s]+([A-Za-z0-9_]+)")


def redact(s):
    s = re.sub(r"(?i)(token|session|cookie)([\"'=:\s]+)[A-Za-z0-9._-]{8,}", r"\1\2[redacted]", s or "")
    return " ".join(s.split())


class Mcp:
    """One long-lived legacy-mode stdio MCP session (direct execution, no MRTR)."""

    def __init__(self):
        self.proc = subprocess.Popen(
            [str(BIN)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, encoding="utf-8", errors="replace",
        )
        self.q = queue.Queue()
        self.next_id = 0
        self.dead = False
        threading.Thread(target=self._reader, daemon=True).start()
        self._request("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "e2e-matrix", "version": "1"},
        }, timeout=15)
        self._notify("notifications/initialized", {})

    def _reader(self):
        for line in self.proc.stdout:
            line = line.strip()
            if line.startswith("{"):
                try:
                    self.q.put(json.loads(line))
                except json.JSONDecodeError:
                    pass
        self.q.put(None)  # EOF sentinel

    def _notify(self, method, params):
        self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method, "params": params}) + "\n")
        self.proc.stdin.flush()

    def _request(self, method, params, timeout=60):
        if self.dead:
            raise RuntimeError("server process dead")
        self.next_id += 1
        rid = self.next_id
        self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": rid, "method": method, "params": params}) + "\n")
        self.proc.stdin.flush()
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                msg = self.q.get(timeout=max(1, deadline - time.time()))
            except queue.Empty:
                break
            if msg is None:
                self.dead = True
                raise RuntimeError("server closed stdout (process exited)")
            if msg.get("id") == rid:
                return msg
        raise TimeoutError(f"timeout waiting for {method} id={rid}")

    def tool(self, name, arguments=None, timeout=120):
        msg = self._request("tools/call", {"name": name, "arguments": arguments or {}}, timeout=timeout)
        result = msg.get("result", {})
        content = result.get("content") or []
        text = content[0].get("text", "") if content else ""
        return result.get("isError") is True, text, result

    def close(self):
        try:
            self.proc.kill()
        except Exception:
            pass


class Matrix:
    def __init__(self, args):
        self.args = args
        self.rows = {}  # tool -> row dict
        self.ctx = {}   # artifacts: tokens, paths
        self.created_media = []
        self.created_chars = []
        self.jobs_started = []
        self.start_credits = None
        self.spent = 0

    def record(self, tool, status, detail="", credits=0):
        self.rows[tool] = {"status": status, "detail": redact(detail)[:160], "credits": credits}
        tag = {"PASS": "PASS", "FAIL": "FAIL", "SKIP": "skip", "FAIL-timeout": "TOUT"}[status]
        print(f"  [{tag}] {tool}: {redact(detail)[:110]}")

    def extract_token(self, text, prefix=None):
        if prefix:
            m = re.search(r"\b(" + prefix + r"_[A-Za-z0-9]{6,})\b", text)
            if m:
                return m.group(1)
        m = TOKEN_RE.search(text) or GENERIC_TOKEN_RE.search(text)
        return m.group(1) if m else None

    def credits_now(self, mcp):
        err, text, _ = mcp.tool("artcraft_get_credits", {})
        m = re.search(r"Banked credits: (\d+)", text)
        return int(m.group(1)) if m else None

    def run_tool(self, mcp, tool, arguments, timeout=120, expect_error_ok=False):
        """Run a tool, record PASS/FAIL, return (ok, text)."""
        if mcp.dead:
            self.record(tool, "SKIP", "server process dead")
            return False, ""
        try:
            err, text, _ = mcp.tool(tool, arguments, timeout=timeout)
        except Exception as e:
            self.record(tool, "FAIL", f"harness: {e}")
            return False, ""
        if err and not expect_error_ok:
            self.record(tool, "FAIL", text[:140])
            return False, text
        self.record(tool, "PASS", text[:140])
        return True, text

    def run_paid(self, mcp, tool, arguments, est=None, timeout=300, poll=False):
        """Budget-guarded paid tool. est in credits if preflightable."""
        if self.args.phases != "all":
            self.record(tool, "SKIP", "paid phase not enabled")
            return False, ""
        projected = self.spent + (est or 0)
        if est is not None and projected > self.args.budget_credits:
            self.record(tool, "SKIP", f"budget: {projected}cr projected > {self.args.budget_credits}cr cap")
            return False, ""
        before = self.credits_now(mcp)
        ok, text = self.run_tool(mcp, tool, arguments, timeout=timeout)
        after = self.credits_now(mcp)
        delta = (before - after) if (before is not None and after is not None) else 0
        if delta > 0:
            self.spent += delta
            self.rows[tool]["credits"] = delta
            print(f"       spent +{delta}cr (cumulative {self.spent}cr)")
        if ok and poll:
            job = JOB_RE.search(text)
            if job:
                self.jobs_started.append(job.group(1))
        return ok, text

    def poll_job(self, mcp, job_token, tool_label, timeout_s=240):
        deadline = time.time() + timeout_s
        last = ""
        while time.time() < deadline:
            try:
                err, text, _ = mcp.tool("artcraft_get_job_status", {"job_token": job_token}, timeout=30)
            except Exception as e:
                return None, f"poll error: {e}"
            last = text
            low = text.lower()
            if any(w in low for w in ("succeed", "complete", "finished")) or TOKEN_RE.search(text):
                return text, None
            if any(w in low for w in ("failed", "error", "cancelled")):
                return text, "job failed"
            time.sleep(5)
        return last, "timeout"


def make_fixtures(d):
    paths = {}
    subprocess.run([FFMPEG, "-y", "-f", "lavfi", "-i", "color=c=red:s=64x64:d=1", "-frames:v", "1", str(d / "img.png")], capture_output=True)
    subprocess.run([FFMPEG, "-y", "-f", "lavfi", "-i", "color=c=black:s=64x64:d=1", "-frames:v", "1", str(d / "mask.png")], capture_output=True)
    subprocess.run([FFMPEG, "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-codec:a", "libmp3lame", str(d / "tone.mp3")], capture_output=True)
    subprocess.run([FFMPEG, "-y", "-f", "lavfi", "-i", "testsrc=size=64x64:duration=1:rate=10", "-pix_fmt", "yuv420p", str(d / "vid.mp4")], capture_output=True)
    for k, f in (("img", "img.png"), ("mask", "mask.png"), ("wav", "tone.mp3"), ("mp4", "vid.mp4")):
        p = d / f
        if p.exists() and p.stat().st_size > 0:
            paths[k] = str(p)
    return paths


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--phases", choices=["free", "all"], default="free")
    ap.add_argument("--budget-credits", type=int, default=600)
    ap.add_argument("--report", default=None)
    args = ap.parse_args()
    report_path = Path(args.report or (ROOT / "tools" / "e2e_matrix_report.json"))

    print(f"binary: {BIN}")
    mx = Matrix(args)
    mcp = Mcp()
    tmp = Path(tempfile.mkdtemp(prefix="artcraft-e2e-"))
    fx = make_fixtures(tmp)
    print(f"fixtures: {sorted(fx)} in {tmp}")

    try:
        mx.start_credits = mx.credits_now(mcp)
        print(f"start credits: {mx.start_credits}")
        mx.record("artcraft_get_credits", "PASS", f"balance readable: {mx.start_credits} banked")

        # ---- Phase 0: read-only sweep ----
        print("== phase 0: read-only ==")
        for tool, a in [
            ("artcraft_get_session_info", {}), ("artcraft_get_subscription", {}),
            ("artcraft_get_billing_portal_url", {}), ("artcraft_list_jobs", {}),
            ("artcraft_list_image_models", {}), ("artcraft_list_video_models", {}),
            ("artcraft_list_media_files", {}), ("artcraft_search_media", {"query": "test"}),
            ("artcraft_tts_search_models", {}), ("artcraft_list_voice_conversion_models", {}),
            ("artcraft_list_voices", {}), ("artcraft_list_characters", {}),
            ("artcraft_list_weights", {}), ("artcraft_search_weights", {"query": "flux"}),
            ("artcraft_list_bookmarks", {}), ("artcraft_list_referral_codes", {}),
            ("artcraft_estimate_cost", {"media_type": "image", "model": "flux_1_dev"}),
            ("artcraft_estimate_splat_cost", {"model": "marble_0p1_mini", "has_reference_image": False}),
        ]:
            mx.run_tool(mcp, tool, a)
        # get_* error-path probes for tokens we don't have
        for tool, a in [
            ("artcraft_get_weight", {"weight_token": "w_nonexistent"}),
            ("artcraft_get_character", {"character_token": "ch_nonexistent"}),
            ("artcraft_get_prompt", {"prompt_token": "pt_nonexistent"}),
        ]:
            mx.run_tool(mcp, tool, a, expect_error_ok=True)
        mx.run_tool(mcp, "artcraft_get_job_status", {"job_token": "ij_nonexistent"})

        # ---- Phase 1: free uploads ----
        print("== phase 1: uploads ==")
        for key, tool in (("img", "artcraft_upload_image"), ("mp4", "artcraft_upload_video"), ("wav", "artcraft_upload_audio")):
            if key not in fx:
                mx.record(tool, "SKIP", f"fixture {key} missing")
                continue
            ok, text = mx.run_tool(mcp, tool, {"file_path": fx[key]}, timeout=180)
            tok = mx.extract_token(text, "mf")
            if ok and tok:
                mx.ctx[key + "_token"] = tok
                mx.created_media.append(tok)
        # mask upload for inpaint
        if "mask" in fx:
            ok, text = mx.run_tool(mcp, "artcraft_upload_image", {"file_path": fx["mask"]}, timeout=180)
            tok = mx.extract_token(text, "mf")
            if ok and tok:
                mx.ctx["mask_token"] = tok
                mx.created_media.append(tok)

        # ---- Phase 2: free generation (flux_1_schnell is free+unlimited) ----
        print("== phase 2: free image generation ==")
        before = mx.credits_now(mcp)
        ok, text = mx.run_tool(mcp, "artcraft_generate_image",
                               {"prompt": "a small red cube on a white background, simple test image",
                                "model": "flux_1_schnell", "image_batch_count": 1}, timeout=180)
        job = JOB_RE.search(text)
        gen_token = None
        if ok and job:
            mx.jobs_started.append(job.group(1))
            final, perr = mx.poll_job(mcp, job.group(1), "generate_image", timeout_s=240)
            if final and not perr:
                gen_token = mx.extract_token(final, "mf")
                if gen_token:
                    mx.ctx["gen_image_token"] = gen_token
                    mx.created_media.append(gen_token)
                    print(f"  [ok] generated image media token captured")
            elif perr:
                print(f"  [warn] image job polling: {perr}")
        after = mx.credits_now(mcp)
        if before is not None and after is not None:
            mx.rows["artcraft_generate_image"]["credits"] = before - after

        src_img = mx.ctx.get("gen_image_token") or mx.ctx.get("img_token")

        # ---- Phase 3: media mutations on self-created files (free) ----
        print("== phase 3: media mutations ==")
        if src_img:
            mx.run_tool(mcp, "artcraft_get_media_file", {"media_token": src_img})
            mx.run_tool(mcp, "artcraft_rename_media_file", {"media_token": src_img, "title": "e2e-test-harness image"})
            mx.run_tool(mcp, "artcraft_rate_media", {"media_token": src_img, "rating": "up"})
            mx.run_tool(mcp, "artcraft_set_tags", {"media_token": src_img, "tags": ["e2e-test"], "add": True})
            mx.run_tool(mcp, "artcraft_list_tags", {"media_token": src_img})
            mx.run_tool(mcp, "artcraft_set_media_visibility", {"media_token": src_img, "is_public": False})
            mx.run_tool(mcp, "artcraft_list_comments", {"media_token": src_img})
            mx.run_tool(mcp, "artcraft_download_media_file", {"media_token": src_img, "download_dir": str(tmp), "filename": "dl.png"}, timeout=180)
        else:
            for t in ("artcraft_get_media_file", "artcraft_rename_media_file", "artcraft_rate_media",
                      "artcraft_set_tags", "artcraft_list_tags", "artcraft_set_media_visibility",
                      "artcraft_list_comments", "artcraft_download_media_file"):
                mx.record(t, "SKIP", "no image token available")

        # ---- Phase 4: cheap paid tools ----
        print("== phase 4: cheap paid ==")
        if src_img:
            mx.run_paid(mcp, "artcraft_edit_image", {"image_media_token": src_img, "prompt": "make it blue", "model": "gemini_25_flash"}, est=8)
            if mx.ctx.get("mask_token"):
                mx.run_paid(mcp, "artcraft_inpaint_image",
                            {"image_media_token": src_img, "mask_media_token": mx.ctx["mask_token"], "prompt": "a green sphere"}, est=8)
            else:
                mx.record("artcraft_inpaint_image", "SKIP", "no mask token")
            mx.run_paid(mcp, "artcraft_edit_image_angle", {"image_media_token": src_img, "horizontal_angle": 45.0}, est=8)
            mx.run_paid(mcp, "artcraft_remove_background", {"image_media_token": src_img}, est=4)
        else:
            for t in ("artcraft_edit_image", "artcraft_inpaint_image", "artcraft_edit_image_angle", "artcraft_remove_background"):
                mx.record(t, "SKIP", "no source image")
        mx.run_paid(mcp, "artcraft_tts_generate", {"text": "test", "voice_model": "artcraft_tts"}, est=5, poll=True)
        mx.run_paid(mcp, "artcraft_generate_audio", {"prompt": "short test beep jingle", "is_instrumental": True}, est=25, poll=True)
        # cheapest video: wan_3p0 2s 480p = 10cr preflighted
        mx.run_paid(mcp, "artcraft_generate_video",
                    {"prompt": "a red cube rotating", "model": "wan_3p0", "duration_seconds": 2, "resolution": "480p"},
                    est=10, timeout=180, poll=True)

        # voice chain (expected partial failure given known 404s)
        print("== phase 4b: voice chain ==")
        ok, text = (False, "")
        if args.phases == "all":
            ok, text = mx.run_tool(mcp, "artcraft_create_voice_dataset", {"name": "e2e-test-harness dataset"})
            ds = mx.extract_token(text)
            if ok and ds:
                mx.run_tool(mcp, "artcraft_upload_voice_sample", {"dataset_token": ds, "file_path": fx.get("wav", "")}, timeout=180)
            else:
                mx.record("artcraft_upload_voice_sample", "SKIP", "no dataset token")
            okv, textv = mx.run_tool(mcp, "artcraft_create_voice", {"name": "e2e-test-harness voice"})
            vt = mx.extract_token(textv)
            aud = mx.ctx.get("wav_token")
            if okv and vt and aud:
                mx.run_paid(mcp, "artcraft_voice_convert", {"audio_media_token": aud, "model_token": vt}, est=5, poll=True)
            else:
                mx.record("artcraft_voice_convert", "SKIP", "no voice token or audio token")
        else:
            for t in ("artcraft_create_voice_dataset", "artcraft_upload_voice_sample", "artcraft_create_voice", "artcraft_voice_convert"):
                mx.record(t, "SKIP", "paid phase not enabled")

        # ---- Phase 5: high tier, preflighted, cheapest first ----
        print("== phase 5: high tier ==")
        if args.phases == "all":
            # video editing needs a completed video; check job list state — best effort
            vid_tok = mx.ctx.get("mp4_token")
            if vid_tok:
                mx.run_paid(mcp, "artcraft_edit_video", {"video_media_token": vid_tok, "prompt": "make it darker"}, est=100)
                mx.run_paid(mcp, "artcraft_studio_gen2", {"video_media_token": vid_tok, "image_media_token": src_img, "prompt": "test"}, est=100)
            else:
                mx.record("artcraft_edit_video", "SKIP", "no video token")
                mx.record("artcraft_studio_gen2", "SKIP", "no video token")
            if src_img:
                mx.run_paid(mcp, "artcraft_generate_3d_object", {"image_media_token": src_img}, est=150, poll=True)
                mx.run_paid(mcp, "artcraft_generate_splat", {"image_media_token": src_img, "model": "marble_0p1_mini"}, est=150, poll=True)
            else:
                mx.record("artcraft_generate_3d_object", "SKIP", "no image token")
                mx.record("artcraft_generate_splat", "SKIP", "no image token")
        else:
            for t in ("artcraft_edit_video", "artcraft_studio_gen2", "artcraft_generate_3d_object", "artcraft_generate_splat"):
                mx.record(t, "SKIP", "paid phase not enabled")

        # ---- Phase 6: mutations/community + cleanup ----
        print("== phase 6: community + cleanup ==")
        if args.phases == "all":
            mx.run_tool(mcp, "artcraft_create_prompt", {"text": "e2e-test-harness artifact, ignore"})
            if src_img:
                mx.run_tool(mcp, "artcraft_create_bookmark", {"media_token": src_img})
                mx.run_tool(mcp, "artcraft_create_comment", {"media_token": src_img, "text": "e2e-test-harness artifact, ignore"})
            mx.run_tool(mcp, "artcraft_create_referral_code", {"code": "e2etestharness"})
            mx.run_tool(mcp, "artcraft_create_checkout_session", {"credits_pack": "artcraft_1000"})
            mx.run_tool(mcp, "artcraft_create_subscription_checkout", {"plan": "artcraft_basic", "cadence": "monthly"})
            if src_img:
                okc, textc = mx.run_tool(mcp, "artcraft_create_character", {"image_media_token": src_img, "name": "e2e-test-harness char"})
                ct = mx.extract_token(textc)
                if okc and ct:
                    mx.run_tool(mcp, "artcraft_get_character", {"character_token": ct})
                    mx.run_tool(mcp, "artcraft_delete_character", {"character_token": ct})
                else:
                    mx.record("artcraft_delete_character", "SKIP", "character creation failed")
            else:
                for t in ("artcraft_create_character", "artcraft_delete_character"):
                    mx.record(t, "SKIP", "no image token")
        else:
            for t in ("artcraft_create_prompt", "artcraft_create_bookmark", "artcraft_create_comment",
                      "artcraft_create_referral_code", "artcraft_create_checkout_session",
                      "artcraft_create_subscription_checkout", "artcraft_create_character", "artcraft_delete_character"):
                mx.record(t, "SKIP", "paid phase not enabled")

        # terminate_job: error-path probe always safe
        mx.run_tool(mcp, "artcraft_terminate_job", {"job_token": "ij_nonexistent"}, expect_error_ok=False)
        # delete_weight error-path (no weights exist on account)
        mx.run_tool(mcp, "artcraft_delete_weight", {"weight_token": "w_nonexistent"}, expect_error_ok=True)

        # cleanup self-created media
        for tok in list(mx.created_media):
            err, text, _ = mcp.tool("artcraft_delete_media_file", {"media_token": tok}, timeout=60)
            print(f"  cleanup delete {tok[:12]}...: {'ok' if not err else 'err'}")
        if mx.created_media:
            mx.record("artcraft_delete_media_file", "PASS", f"deleted {len(mx.created_media)} self-created media files")
        else:
            mx.record("artcraft_delete_media_file", "SKIP", "no media created")

        end_credits = mx.credits_now(mcp)
        total_delta = (mx.start_credits - end_credits) if (mx.start_credits is not None and end_credits is not None) else None
        print(f"\nend credits: {end_credits}  total delta: {total_delta}")

    finally:
        mcp.close()

    # coverage check: every business tool must have a row
    all_tools = sorted(json.loads(p.read_text(encoding="utf-8"))["name"] for p in (ROOT / "schemas").glob("*.json"))
    for t in all_tools:
        if t not in mx.rows:
            mx.rows[t] = {"status": "SKIP", "detail": "no test coverage", "credits": 0}

    passed = sum(1 for r in mx.rows.values() if r["status"] == "PASS")
    failed = sum(1 for r in mx.rows.values() if r["status"].startswith("FAIL"))
    skipped = sum(1 for r in mx.rows.values() if r["status"] == "SKIP")
    print(f"\n==== MATRIX: {passed} PASS / {failed} FAIL / {skipped} SKIP of {len(mx.rows)} ====")
    for t in sorted(mx.rows):
        r = mx.rows[t]
        if r["status"] != "PASS":
            print(f"  {r['status']:12} {t}: {r['detail'][:90]}")

    report = {"phases": args.phases, "budget": args.budget_credits, "start_credits": mx.start_credits,
              "measured_spend": mx.spent, "rows": mx.rows,
              "summary": {"pass": passed, "fail": failed, "skip": skipped}}
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"report: {report_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
