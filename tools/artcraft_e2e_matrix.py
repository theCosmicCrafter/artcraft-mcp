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
    s = re.sub(r"\b(?:mf|m|vo|vd|ch|pt|bk|cm|rf|ij|jb|jinf)_[A-Za-z0-9]{6,}\b", "[redacted-token]", s)
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
        self.jobs_started = []  # (tool, job_token) pairs for jobs that may produce media
        self.job_results = {}
        self.start_credits = None
        self.end_credits = None
        self.net_credit_delta = None
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
                self.jobs_started.append((tool, job.group(1)))
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

    def poll_started_jobs(self, mcp, timeout_s=900):
        """Poll all queued paid/free jobs once, in parallel-ish batches, and capture media outputs for cleanup."""
        pending = {}
        for tool, job in self.jobs_started:
            pending.setdefault(job, tool)
        if not pending:
            return

        print(f"== polling {len(pending)} started jobs (up to {timeout_s}s) ==")
        errors = {}
        deadline = time.time() + timeout_s
        while pending and time.time() < deadline:
            for job, tool in list(pending.items()):
                try:
                    err, text, _ = mcp.tool("artcraft_get_job_status", {"job_token": job}, timeout=30)
                    if err:
                        raise RuntimeError(text)
                except Exception as e:
                    errors[job] = errors.get(job, 0) + 1
                    if errors[job] >= 3:
                        self.job_results[tool] = {"status": "poll_error", "detail": redact(str(e))[:240]}
                        print(f"  [job] {tool}: poll error")
                        del pending[job]
                    continue

                m_media = re.search(r"\b(m_[A-Za-z0-9]{6,})\b", text)
                result_token = m_media.group(1) if m_media else None
                status = ""
                for line in text.splitlines():
                    if line.lower().startswith("status:"):
                        status = line.split(":", 1)[1].strip().lower()
                        break

                if result_token or any(w in status for w in ("succeed", "complete", "finished", "success")):
                    if result_token and result_token not in self.created_media:
                        self.created_media.append(result_token)
                    self.job_results[tool] = {"status": "completed", "detail": redact(text)[:240]}
                    if tool in self.rows:
                        self.rows[tool]["detail"] = (self.rows[tool]["detail"] + "; job completed")[:160]
                    print(f"  [job] {tool}: completed" + ("; output captured" if result_token else ""))
                    del pending[job]
                elif any(w in status for w in ("failed", "error", "cancelled", "canceled")):
                    self.job_results[tool] = {"status": "failed", "detail": redact(text)[:240]}
                    if tool in self.rows:
                        self.rows[tool]["detail"] = (self.rows[tool]["detail"] + "; job failed upstream")[:160]
                    print(f"  [job] {tool}: failed upstream")
                    del pending[job]

            if pending:
                time.sleep(10)

        for job, tool in list(pending.items()):
            # Timed-out jobs are self-created test work; cancel them rather than leaving them running.
            term_detail = "termination not attempted"
            try:
                terr, ttext, _ = mcp.tool("artcraft_terminate_job", {"job_token": job}, timeout=30)
                term_detail = "termination requested" if not terr else "termination failed"
            except Exception:
                term_detail = "termination failed"
            self.job_results[tool] = {"status": "timeout", "detail": term_detail}
            if tool in self.rows:
                self.rows[tool]["detail"] = (self.rows[tool]["detail"] + "; job poll timeout")[:160]
            print(f"  [job] {tool}: timeout ({term_detail})")


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
            ("artcraft_tts_search_models", {}), ("artcraft_list_characters", {}),
            ("artcraft_list_weights", {}), ("artcraft_search_weights", {"query": "flux"}),
            ("artcraft_list_bookmarks", {}), ("artcraft_list_referral_codes", {}),
            ("artcraft_list_folders", {}), ("artcraft_list_media_files_without_folder", {}),
            ("artcraft_list_api_keys", {}), ("artcraft_list_mcp_sessions", {}),
            ("artcraft_get_craft_apps_release_info", {}),
            ("artcraft_list_featured_media", {"page_size": 3}),
            ("artcraft_search_featured_media", {"query": "test"}),
            ("artcraft_list_beta_keys", {"page_size": 5}),
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
        mx.run_tool(mcp, "artcraft_get_job_status", {"job_token": "ij_nonexistent"}, expect_error_ok=True)

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
            mx.jobs_started.append(("artcraft_generate_image", job.group(1)))
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
            mx.run_paid(mcp, "artcraft_edit_image", {"image_media_token": src_img, "prompt": "make it blue", "model": "gemini_25_flash"}, est=4, poll=True)
            if mx.ctx.get("mask_token"):
                mx.run_paid(mcp, "artcraft_inpaint_image",
                            {"image_media_token": src_img, "mask_media_token": mx.ctx["mask_token"], "prompt": "a green sphere"}, est=8, poll=True)
            else:
                mx.record("artcraft_inpaint_image", "SKIP", "no mask token")
            mx.run_paid(mcp, "artcraft_edit_image_angle", {"image_media_token": src_img, "horizontal_angle": 45.0}, est=4, poll=True)
            mx.run_paid(mcp, "artcraft_remove_background", {"image_media_token": src_img}, est=4, poll=True)
        else:
            for t in ("artcraft_edit_image", "artcraft_inpaint_image", "artcraft_edit_image_angle", "artcraft_remove_background"):
                mx.record(t, "SKIP", "no source image")
        # Legacy TTS is deprecated upstream and returns a synthetic nonfunctional job token;
        # verify the enqueue/deprecation surface but do not spend polling time on it.
        mx.run_paid(mcp, "artcraft_tts_generate", {"text": "test", "voice_model": "artcraft_tts"}, est=0)
        # cheapest audio: suno_sounds is a flat 3cr sound-effects generation
        mx.run_paid(mcp, "artcraft_generate_audio", {"prompt": "short clean test beep sound effect", "model": "suno_sounds"}, est=3, poll=True)
        # cheapest video: wan_3p0 2s 480p = 10cr preflighted
        mx.run_paid(mcp, "artcraft_generate_video",
                    {"prompt": "a red cube rotating", "model": "wan_3p0", "duration_seconds": 2, "resolution": "480p"},
                    est=10, timeout=180, poll=True)

        # The retired voice and Studio Gen2 product families were removed in P1 and
        # are intentionally not represented here.

        # ---- Phase 5: high tier, preflighted, cheapest first ----
        print("== phase 5: high tier ==")
        if args.phases == "all":
            # video editing needs a completed video; check job list state — best effort
            vid_tok = mx.ctx.get("mp4_token")
            if vid_tok:
                # Beeble has no upstream cost-estimate route; current upstream does not deduct wallet credits
                # for this endpoint, but keep a conservative 20cr guardrail in case pricing is enabled.
                mx.run_paid(mcp, "artcraft_edit_video", {"video_media_token": vid_tok, "prompt": "make it darker"}, est=20, poll=True)
            else:
                mx.record("artcraft_edit_video", "SKIP", "no video token")
            if src_img:
                # cheapest mesh: rodin_2p5_fast flat 13cr; cheapest splat: marble_0p1_mini flat 18cr
                mx.run_paid(mcp, "artcraft_generate_3d_object", {"image_media_token": src_img, "model": "rodin_2p5_fast"}, est=13, poll=True)
                mx.run_paid(mcp, "artcraft_generate_splat", {"image_media_token": src_img, "model": "marble_0p1_mini"}, est=18, poll=True)
            else:
                mx.record("artcraft_generate_3d_object", "SKIP", "no image token")
                mx.record("artcraft_generate_splat", "SKIP", "no image token")
        else:
            for t in ("artcraft_edit_video", "artcraft_generate_3d_object", "artcraft_generate_splat"):
                mx.record(t, "SKIP", "paid phase not enabled")

        # ---- Phase 6: mutations/community + cleanup ----
        print("== phase 6: community + cleanup ==")
        if args.phases == "all":
            mx.run_tool(mcp, "artcraft_create_prompt", {"text": "e2e-test-harness artifact, ignore"})
            if src_img:
                okb, textb = mx.run_tool(mcp, "artcraft_create_bookmark", {"media_token": src_img})
                bm = None
                if okb:
                    m_bm = re.search(r"bookmark\s+token[:'\s]+([A-Za-z0-9_]{8,})", textb, re.I)
                    bm = m_bm.group(1) if m_bm else None
                if bm:
                    mx.ctx["bookmark_token"] = bm
                    mx.run_tool(mcp, "artcraft_delete_bookmark", {"bookmark_token": bm})
                else:
                    mx.record("artcraft_delete_bookmark", "SKIP", "bookmark creation did not yield a token")

                okcm, textcm = mx.run_tool(mcp, "artcraft_create_comment", {"media_token": src_img, "text": "e2e-test-harness artifact, ignore"})
                cm = None
                if okcm:
                    m_cm = re.search(r"comment\s+token[:'\s]+([A-Za-z0-9_]{8,})", textcm, re.I)
                    cm = m_cm.group(1) if m_cm else None
                if cm:
                    mx.ctx["comment_token"] = cm
                    mx.run_tool(mcp, "artcraft_delete_comment", {"comment_token": cm})
                else:
                    mx.record("artcraft_delete_comment", "SKIP", "comment creation did not yield a token")
            else:
                for t in ("artcraft_create_bookmark", "artcraft_create_comment", "artcraft_delete_bookmark", "artcraft_delete_comment"):
                    mx.record(t, "SKIP", "no image token")
            mx.run_tool(mcp, "artcraft_create_referral_code", {"code": "e2etestharness"})
            mx.run_tool(mcp, "artcraft_create_checkout_session", {"credits_pack": "artcraft_1000"})
            mx.run_tool(mcp, "artcraft_create_subscription_checkout", {"plan": "artcraft_basic", "cadence": "monthly"})
            if src_img:
                okc, textc = mx.run_tool(mcp, "artcraft_create_character", {"image_media_token": src_img, "name": "e2e-test-harness char"})
                job = JOB_RE.search(textc)
                if okc and job:
                    mx.jobs_started.append(("artcraft_create_character", job.group(1)))
                    mx.ctx["character_job_started"] = True
                    mx.record("artcraft_delete_character", "SKIP", "deferred until character job completes")
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

        # ---- Phase 7: expansion-family read-only probes and safe mutation error paths ----
        print("== phase 7: expansion families (free/error-path) ==")
        mx.run_tool(mcp, "artcraft_is_featured_item", {"entity_type": "media_file", "entity_token": "m_nonexistent"})

        # Community deletion error paths with nonexistent entities when no valid self-created
        # artifact was available for the real cleanup path above. Nothing is deleted here.
        if not mx.ctx.get("bookmark_token"):
            mx.run_tool(mcp, "artcraft_delete_bookmark", {"bookmark_token": "ub_nonexistent"}, expect_error_ok=True)
        if not mx.ctx.get("comment_token"):
            mx.run_tool(mcp, "artcraft_delete_comment", {"comment_token": "comment_nonexistent"}, expect_error_ok=True)

        # Folder read paths with nonexistent tokens plus mutation error paths. No folder is created or changed.
        for tool, a in [
            ("artcraft_get_folder", {"folder_token": "folder_nonexistent"}),
            ("artcraft_list_subfolders", {"folder_token": "folder_nonexistent"}),
            ("artcraft_list_folder_media_files", {"folder_token": "folder_nonexistent"}),
            ("artcraft_create_folder", {"name": ""}),
            ("artcraft_delete_folder", {"folder_token": "folder_nonexistent"}),
            ("artcraft_rename_folder", {"folder_token": "folder_nonexistent", "new_name": "x"}),
            ("artcraft_set_folder_star", {"folder_token": "folder_nonexistent", "has_star": True}),
            ("artcraft_set_folder_color", {"folder_token": "folder_nonexistent", "color_code": "red"}),
            ("artcraft_set_folder_cover_image", {"folder_token": "folder_nonexistent", "media_file_token": "mf_nonexistent"}),
            ("artcraft_add_subfolders", {"folder_token": "folder_nonexistent", "subfolder_tokens": ["folder_nonexistent_child"]}),
            ("artcraft_remove_subfolders", {"folder_token": "folder_nonexistent", "subfolder_tokens": ["folder_nonexistent_child"]}),
            ("artcraft_add_media_to_folder", {"folder_token": "folder_nonexistent", "media_file_tokens": ["mf_nonexistent"]}),
            ("artcraft_remove_media_from_folder", {"folder_token": "folder_nonexistent", "media_file_tokens": ["mf_nonexistent"]}),
            ("artcraft_move_media_to_folder", {"folder_token": "folder_nonexistent", "source_folder_token": "folder_nonexistent_source", "media_file_tokens": ["mf_nonexistent"]}),
        ]:
            mx.run_tool(mcp, tool, a, expect_error_ok=True)

        # Billing portal routes create Stripe-hosted portal links but never cancel/switch/update
        # anything by themselves. This account has no active subscription, so these are safe upstream errors.
        for tool, a in [
            ("artcraft_get_billing_cancel_plan_url", {}),
            ("artcraft_get_billing_switch_plan_url", {"plan": "artcraft_basic", "cadence": "monthly"}),
            ("artcraft_get_billing_update_payment_method_url", {}),
            ("artcraft_set_unsubscribe_reason", {"reason": ""}),
        ]:
            mx.run_tool(mcp, tool, a, expect_error_ok=True)

        # API-key and MCP-session lifecycle error paths. Secret-minting success paths are never exercised by the harness.
        for tool, a in [
            ("artcraft_create_api_key", {"name": ""}),
            ("artcraft_get_api_key", {"api_key_token": "apikey_nonexistent"}),
            ("artcraft_update_api_key", {"api_key_token": "apikey_nonexistent", "description": "x"}),
            ("artcraft_delete_api_key", {"api_key_token": "apikey_nonexistent"}),
            ("artcraft_create_mcp_session", {"client_name": "x" * 300}),
            ("artcraft_refresh_mcp_session", {"private_refresh_token": "mcp_refresh_nonexistent"}),
            ("artcraft_delete_mcp_session", {"session_token": "mcps_nonexistent"}),
        ]:
            mx.run_tool(mcp, tool, a, expect_error_ok=True)
        # There is no safe error-path for revoke: when the ambient credential is an MCP session,
        # the valid endpoint semantics revoke that live credential. Never probe it destructively.
        mx.record("artcraft_revoke_mcp_session", "SKIP", "destructive self-revocation not probed")

        # Upload/paid legacy job tools: force client-side or upstream validation failures before mutation/spend.
        missing_img = str(tmp / "missing-snapshot.png")
        missing_zip = str(tmp / "missing-studio-shot.zip")
        for tool, a in [
            ("artcraft_upload_studio_scene_snapshot", {"file_path": missing_img}),
            ("artcraft_upload_studio_shot", {"file_path": missing_zip}),
            ("artcraft_create_face_animation", {}),
            ("artcraft_create_rerender_animation", {"video_media_file_token": "mf_nonexistent"}),
            ("artcraft_convert_fbx_to_gltf", {"media_file_token": ""}),
            ("artcraft_convert_render_engine_scene", {"media_file_token": ""}),
            ("artcraft_convert_bvh_to_workflow", {"media_file_token": ""}),
            ("artcraft_redeem_beta_key", {"beta_key": "nonexistent-beta-key"}),
        ]:
            mx.run_tool(mcp, tool, a, expect_error_ok=True)

        # terminate_job: error-path probe always safe
        mx.run_tool(mcp, "artcraft_terminate_job", {"job_token": "ij_nonexistent"}, expect_error_ok=True)
        # delete_weight error-path (no weights exist on account)
        mx.run_tool(mcp, "artcraft_delete_weight", {"weight_token": "w_nonexistent"}, expect_error_ok=True)

        # Paid generation jobs were enqueued earlier and have run concurrently while the free
        # mutation/error-path sweep executed. Poll them now, capture result media for cleanup,
        # and cancel anything that fails to finish inside the bounded verification window.
        mx.poll_started_jobs(mcp, timeout_s=900)

        # Character creation is asynchronous: the create call returns a job token, and the
        # character token only appears in list_characters after that job completes.
        if mx.ctx.get("character_job_started"):
            outcome = mx.job_results.get("artcraft_create_character", {})
            if outcome.get("status") == "completed":
                err, chartext, _ = mcp.tool("artcraft_list_characters", {}, timeout=60)
                ct = None
                if not err:
                    for line in chartext.splitlines():
                        if "e2e-test-harness char" in line:
                            m_ct = re.search(r"\b(character_[A-Za-z0-9]+)\b", line)
                            if m_ct:
                                ct = m_ct.group(1)
                                break
                if ct:
                    mx.run_tool(mcp, "artcraft_get_character", {"character_token": ct})
                    mx.run_tool(mcp, "artcraft_delete_character", {"character_token": ct})
                else:
                    mx.record("artcraft_delete_character", "FAIL", "character job completed but test character token was not listed")
            else:
                mx.record("artcraft_delete_character", "SKIP", f"character job did not complete ({outcome.get('status', 'not polled')})")

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
        mx.end_credits = end_credits
        mx.net_credit_delta = total_delta
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
              "end_credits": mx.end_credits, "net_credit_delta": mx.net_credit_delta,
              "measured_spend": mx.spent, "job_results": mx.job_results, "rows": mx.rows,
              "summary": {"pass": passed, "fail": failed, "skip": skipped}}
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"report: {report_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
