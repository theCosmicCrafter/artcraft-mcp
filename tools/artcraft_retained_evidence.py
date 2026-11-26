#!/usr/bin/env python3
"""Retained-artifact evidence run for the ArtCraft MCP server.

Creates real generation artifacts, downloads them into the verification folder,
and records redacted call/job/credit evidence. This script intentionally does
not delete generated media. It deletes only tiny scratch uploads after their
dependent error-path probes are finished.
"""

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlparse

sys.path.insert(0, str(Path(__file__).resolve().parent))
from artcraft_e2e_matrix import BIN, Mcp, make_fixtures  # noqa: E402

JOB_RE = re.compile(r"[Jj]ob token[:'\s]+([A-Za-z0-9_]+)")
MEDIA_RE = re.compile(r"\b((?:m|mf|media_file)_[A-Za-z0-9]{6,})\b")
CHARACTER_RE = re.compile(r"\b(character_[A-Za-z0-9]{6,})\b")
FOLDER_RE = re.compile(r"\b(folder_[A-Za-z0-9]{6,})\b")

SECRET_PATTERNS = [
    (re.compile(r"(?i)(secret api key[^\n:]*:\s*)(\S+)"), r"\1[REDACTED-SECRET]"),
    (re.compile(r"(?i)(private session token[^\n:]*:\s*)(\S+)"), r"\1[REDACTED-SECRET]"),
    (re.compile(r"(?i)(private refresh token[^\n:]*:\s*)(\S+)"), r"\1[REDACTED-SECRET]"),
    (re.compile(r"(?i)(new private refresh token:\s*)(\S+)"), r"\1[REDACTED-SECRET]"),
    (re.compile(r"(?i)(authorization\s*[:=]\s*bearer\s+)(\S+)"), r"\1[REDACTED-SECRET]"),
    (re.compile(r"(?i)(cookie\s*[:=]\s*)([^\n]+)"), r"\1[REDACTED-COOKIE]"),
]


def utcnow():
    return datetime.now(timezone.utc).isoformat()


def redact_text(text):
    text = text or ""
    for pattern, repl in SECRET_PATTERNS:
        text = pattern.sub(repl, text)
    text = text.replace(str(Path.home()), "<USERPROFILE>")
    return text


def parse_credits(text):
    m = re.search(r"Banked credits:\s*(\d+)", text or "")
    return int(m.group(1)) if m else None


def parse_job_status(text):
    status = ""
    progress = None
    result_token = None
    failure = None
    extra = None
    for line in (text or "").splitlines():
        stripped = line.strip()
        lower = stripped.lower()
        if lower.startswith("status:"):
            status = stripped.split(":", 1)[1].strip()
        elif lower.startswith("progress:"):
            num = re.search(r"(\d+)", stripped)
            progress = int(num.group(1)) if num else None
        elif lower.startswith("result token:"):
            result_token = stripped.split(":", 1)[1].strip()
        elif lower.startswith("failure:"):
            failure = stripped.split(":", 1)[1].strip()
        elif lower.startswith("extra:"):
            extra = stripped.split(":", 1)[1].strip()
    if not result_token:
        m = MEDIA_RE.search(text or "")
        result_token = m.group(1) if m else None
    normalized = status.lower().replace("_", "").replace("-", "").replace(" ", "")
    terminal_success = normalized in {"completesuccess", "complete", "succeeded", "success", "finished"} or bool(result_token)
    terminal_failure = normalized in {"completefailure", "failed", "error", "dead", "cancelledbyuser", "canceledbyuser", "cancelledbysystem", "canceledbysystem"}
    return {
        "status": status,
        "normalized": normalized,
        "progress": progress,
        "result_token": result_token,
        "failure": failure,
        "extra": extra,
        "terminal_success": terminal_success,
        "terminal_failure": terminal_failure,
    }


def first_token(text, regex=None):
    if regex:
        m = regex.search(text or "")
        if m:
            return m.group(1)
    m = re.search(r"(?:Token|token)[:'\s]+([A-Za-z0-9_]{8,})", text or "")
    return m.group(1) if m else None


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


class EvidenceRun:
    def __init__(self, verification_dir, out_path, audio_stall_seconds=600, job_timeout_seconds=1200):
        self.verification_dir = Path(verification_dir)
        self.verification_dir.mkdir(parents=True, exist_ok=True)
        self.out_path = Path(out_path)
        self.audio_stall_seconds = audio_stall_seconds
        self.job_timeout_seconds = job_timeout_seconds
        self.mcp = Mcp()
        self.calls = {}
        self.jobs = {}
        self.artifacts = []
        self.retained_media_tokens = []
        self.scratch_uploads = []
        self.wallet_events = []
        self.upstream_failures = {}
        self.started_at = utcnow()
        self.start_credits = None
        self.end_credits = None
        self.measured_positive_spend = 0
        self.evidence = {
            "schema": "artcraft-retained-evidence-v1",
            "started_at": self.started_at,
            "binary": str(BIN).replace(str(Path.home()), "<USERPROFILE>"),
            "verification_dir": str(self.verification_dir).replace(str(Path.home()), "<USERPROFILE>"),
            "calls": self.calls,
            "jobs": self.jobs,
            "artifacts": self.artifacts,
            "retained_media_tokens": self.retained_media_tokens,
            "scratch_uploads": self.scratch_uploads,
            "wallet_events": self.wallet_events,
            "upstream_failures": self.upstream_failures,
        }

    def save(self):
        wallet_delta = None
        if self.start_credits is not None and self.end_credits is not None:
            wallet_delta = self.start_credits - self.end_credits
        per_call_delta_sum = sum(e.get("delta", 0) for e in self.wallet_events)
        positive_delta_sum = sum(e.get("delta", 0) for e in self.wallet_events if e.get("delta", 0) > 0)
        self.evidence.update({
            "finished_at": utcnow(),
            "start_credits": self.start_credits,
            "end_credits": self.end_credits,
            "wallet_delta": wallet_delta,
            "per_call_delta_sum": per_call_delta_sum,
            "measured_positive_spend": positive_delta_sum,
            "artifact_count": len([a for a in self.artifacts if a.get("downloaded")]),
        })
        self.out_path.parent.mkdir(parents=True, exist_ok=True)
        self.out_path.write_text(json.dumps(self.evidence, indent=2, ensure_ascii=False), encoding="utf-8")

    def credits_now(self):
        err, text, _ = self.mcp.tool("artcraft_get_credits", {}, timeout=60)
        if err:
            return None
        return parse_credits(text)

    def call(self, tool, arguments=None, timeout=180, measure_credits=True, note=None):
        arguments = arguments or {}
        before = self.credits_now() if measure_credits else None
        started = time.time()
        try:
            err, text, result = self.mcp.tool(tool, arguments, timeout=timeout)
            transport_error = None
        except Exception as exc:  # keep evidence even for harness/transport failures
            err, text, result = True, f"Harness exception: {exc}", {}
            transport_error = str(exc)
        elapsed = time.time() - started
        after = self.credits_now() if measure_credits else None
        delta = (before - after) if before is not None and after is not None else 0
        entry = {
            "at": utcnow(),
            "tool": tool,
            "arguments": json.loads(json.dumps(arguments)),
            "is_error": bool(err),
            "text": redact_text(text),
            "text_excerpt": redact_text(text)[:1200],
            "elapsed_seconds": round(elapsed, 3),
            "credits_before": before,
            "credits_after": after,
            "credit_delta": delta,
        }
        if note:
            entry["note"] = note
        if transport_error:
            entry["transport_error"] = transport_error
        self.calls.setdefault(tool, []).append(entry)
        if measure_credits and before is not None and after is not None:
            event = {
                "tool": tool,
                "before": before,
                "after": after,
                "delta": delta,
                "at": entry["at"],
            }
            self.wallet_events.append(event)
            if delta > 0:
                self.measured_positive_spend += delta
        self.save()
        return err, text, entry

    def extract_job(self, text):
        m = JOB_RE.search(text or "")
        return m.group(1) if m else None

    def register_job(self, job_token, tool, artifact_base=None, timeout_seconds=None):
        if not job_token:
            return
        self.jobs[job_token] = {
            "job_token": job_token,
            "tool": tool,
            "artifact_base": artifact_base,
            "registered_at": utcnow(),
            "timeout_seconds": timeout_seconds or self.job_timeout_seconds,
            "poll_count": 0,
            "status_events": [],
            "terminal": False,
            "outcome": "pending",
        }

    def poll_jobs(self, job_tokens, timeout_seconds=None):
        pending = {j: time.time() + (timeout_seconds or self.jobs[j].get("timeout_seconds") or self.job_timeout_seconds) for j in job_tokens if j in self.jobs}
        last_sig = {}
        while pending:
            now = time.time()
            for job, deadline in list(pending.items()):
                meta = self.jobs[job]
                err, text, _ = self.mcp.tool("artcraft_get_job_status", {"job_token": job}, timeout=60)
                meta["poll_count"] = int(meta.get("poll_count", 0)) + 1
                parsed = parse_job_status(text)
                sig = (parsed["status"], parsed["progress"], bool(parsed["result_token"]), err)
                if sig != last_sig.get(job):
                    meta["status_events"].append({
                        "at": utcnow(),
                        "is_error": bool(err),
                        "status": parsed["status"],
                        "progress": parsed["progress"],
                        "result_token": parsed["result_token"],
                        "text_excerpt": redact_text(text)[:900],
                    })
                    last_sig[job] = sig
                    self.save()
                meta["last_status"] = parsed
                meta["last_text"] = redact_text(text)

                if parsed["terminal_success"]:
                    meta["terminal"] = True
                    meta["outcome"] = "completed"
                    meta["completed_at"] = utcnow()
                    meta["result_token"] = parsed["result_token"]
                    if parsed["result_token"] and parsed["result_token"] not in self.retained_media_tokens:
                        self.retained_media_tokens.append(parsed["result_token"])
                    del pending[job]
                    self.save()
                    continue
                if parsed["terminal_failure"]:
                    meta["terminal"] = True
                    meta["outcome"] = "failed"
                    meta["completed_at"] = utcnow()
                    del pending[job]
                    self.save()
                    continue

                # Audio stall evidence: upstream previously left audio jobs at 0%.
                registered = datetime.fromisoformat(meta["registered_at"].replace("Z", "+00:00")).timestamp()
                if meta["tool"] == "artcraft_generate_audio" and now - registered >= self.audio_stall_seconds:
                    if (parsed["progress"] or 0) == 0 and not parsed["result_token"]:
                        meta["terminal"] = False
                        meta["outcome"] = "stalled_pending_zero_percent_after_10min"
                        meta["stall_evidence"] = {
                            "elapsed_seconds": round(now - registered, 1),
                            "status": parsed["status"],
                            "progress": parsed["progress"],
                            "text_excerpt": redact_text(text)[:1200],
                            "note": "Left running upstream; no local termination request was sent.",
                        }
                        del pending[job]
                        self.save()
                        continue

                if now >= deadline:
                    meta["outcome"] = "poll_timeout_left_running"
                    meta["timeout_at"] = utcnow()
                    del pending[job]
                    self.save()
                    continue
            if pending:
                time.sleep(8)

    def media_ext(self, media_token):
        err, text, _ = self.call("artcraft_get_media_file", {"media_token": media_token}, timeout=90, measure_credits=False)
        ext = "bin"
        if not err:
            m = re.search(r"CDN URL:\s*(\S+)", text or "")
            if m:
                path = urlparse(m.group(1)).path
                suffix = Path(path).suffix.lower().lstrip(".")
                if suffix and re.fullmatch(r"[a-z0-9]{1,8}", suffix):
                    ext = suffix
        return ext

    def download_media(self, media_token, base_name, source_tool, job_token=None):
        ext = self.media_ext(media_token)
        filename = f"{base_name}.{ext}"
        err, text, entry = self.call(
            "artcraft_download_media_file",
            {"media_token": media_token, "download_dir": str(self.verification_dir), "filename": filename},
            timeout=240,
            measure_credits=False,
        )
        path = self.verification_dir / filename
        artifact = {
            "kind": base_name,
            "source_tool": source_tool,
            "job_token": job_token,
            "media_token": media_token,
            "filename": filename,
            "downloaded": bool(path.exists() and path.stat().st_size > 0),
            "download_error": bool(err),
            "download_text_excerpt": redact_text(text)[:900],
        }
        if path.exists():
            artifact.update({
                "size_bytes": path.stat().st_size,
                "sha256": sha256_file(path),
            })
        self.artifacts.append(artifact)
        if media_token not in self.retained_media_tokens:
            self.retained_media_tokens.append(media_token)
        self.save()
        return artifact

    def close(self):
        self.end_credits = self.credits_now()
        self.save()
        self.mcp.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--verification-dir", default=r"C:\Users\richk\Downloads\artcraft-mcp-verification")
    ap.add_argument("--out", default=None)
    ap.add_argument("--audio-stall-seconds", type=int, default=600)
    ap.add_argument("--job-timeout-seconds", type=int, default=1200)
    args = ap.parse_args()

    verification_dir = Path(args.verification_dir)
    out = Path(args.out) if args.out else verification_dir / "part1_retained_evidence.json"
    run = EvidenceRun(verification_dir, out, args.audio_stall_seconds, args.job_timeout_seconds)
    tmp = Path(tempfile.mkdtemp(prefix="artcraft-retained-fixtures-"))
    fixtures = make_fixtures(tmp)

    try:
        run.start_credits = run.credits_now()
        run.evidence["fixture_dir"] = str(tmp).replace(str(Path.home()), "<USERPROFILE>")
        run.evidence["start_credits"] = run.start_credits
        run.save()
        print(f"start credits: {run.start_credits}")

        # Tiny scratch uploads used only by the inpaint/edit-video upstream-error probes.
        scratch_tokens = {}
        if fixtures.get("mask"):
            err, text, _entry = run.call("artcraft_upload_image", {"file_path": fixtures["mask"]}, timeout=180, measure_credits=False, note="scratch mask upload for inpaint probe")
            tok = first_token(text, MEDIA_RE)
            if not err and tok:
                scratch_tokens["mask"] = tok
                run.scratch_uploads.append({"kind": "inpaint_mask", "media_token": tok, "deleted": False})
        if fixtures.get("mp4"):
            err, text, _entry = run.call("artcraft_upload_video", {"file_path": fixtures["mp4"]}, timeout=180, measure_credits=False, note="scratch video upload for edit_video probe")
            tok = first_token(text, MEDIA_RE)
            if not err and tok:
                scratch_tokens["video"] = tok
                run.scratch_uploads.append({"kind": "edit_video_input", "media_token": tok, "deleted": False})

        # Three varied free flux_1_schnell generations.
        free_specs = [
            ("01_flux_red_cube", "a small matte red cube centered on a seamless white background, minimal product-render test image"),
            ("02_flux_blue_teapot", "a cobalt blue ceramic teapot on a light gray studio background, simple object reference, centered"),
            ("03_flux_green_robot", "a friendly small green toy robot standing on a clean white surface, simple character reference"),
        ]
        free_jobs = []
        for base, prompt in free_specs:
            err, text, entry = run.call("artcraft_generate_image", {
                "prompt": prompt,
                "model": "flux_1_schnell",
                "image_batch_count": 1,
                "aspect_ratio": "square",
            }, timeout=180, note="free flux_1_schnell retained image")
            job = run.extract_job(text)
            entry["job_token"] = job
            if not err and job:
                run.register_job(job, "artcraft_generate_image", artifact_base=base, timeout_seconds=600)
                free_jobs.append(job)
        run.save()
        run.poll_jobs(free_jobs, timeout_seconds=600)

        image_tokens = {}
        for job in free_jobs:
            meta = run.jobs[job]
            if meta.get("result_token"):
                image_tokens[meta["artifact_base"]] = meta["result_token"]
                run.download_media(meta["result_token"], meta["artifact_base"], "artcraft_generate_image", job)

        source_base = "01_flux_red_cube" if "01_flux_red_cube" in image_tokens else (sorted(image_tokens)[0] if image_tokens else None)
        source_token = image_tokens.get(source_base)
        run.evidence["source_image_for_edits"] = {"base": source_base, "media_token": source_token}
        run.save()
        if not source_token:
            raise RuntimeError("no retained free image completed; cannot run dependent edit/3D/character probes")

        dependent_jobs = []

        def enqueue_job_tool(tool, arguments, artifact_base, note, timeout_seconds=1200):
            err, text, entry = run.call(tool, arguments, timeout=240, note=note)
            job = run.extract_job(text)
            entry["job_token"] = job
            if not err and job:
                run.register_job(job, tool, artifact_base=artifact_base, timeout_seconds=timeout_seconds)
                dependent_jobs.append(job)
            return err, text, job

        # Successful low-cost image transformations.
        enqueue_job_tool("artcraft_edit_image", {
            "image_media_token": source_token,
            "prompt": "recolor the red cube sapphire blue while preserving the simple white studio background",
            "model": "gemini_25_flash",
        }, "04_gemini_edit_blue_cube", "gemini_25_flash image edit retained output")
        enqueue_job_tool("artcraft_edit_image_angle", {
            "image_media_token": source_token,
            "horizontal_angle": 45.0,
        }, "05_angle_edit_45deg", "45-degree angle edit retained output")
        enqueue_job_tool("artcraft_remove_background", {
            "image_media_token": source_token,
        }, "06_background_removed", "background removal retained output")

        # Expected upstream-500 inpaint attempt. Exact evidence is preserved either way.
        err, text, entry = run.call("artcraft_inpaint_image", {
            "image_media_token": source_token,
            "mask_media_token": scratch_tokens.get("mask", "m_missing_mask"),
            "prompt": "replace the masked area with a small green sphere",
        }, timeout=180, note="expected upstream HTTP 500; scratch mask input")
        job = run.extract_job(text)
        entry["job_token"] = job
        if err:
            run.upstream_failures["artcraft_inpaint_image"] = {
                "expected": "upstream HTTP 500",
                "exact_error": redact_text(text),
                "credits_before": entry.get("credits_before"),
                "credits_after": entry.get("credits_after"),
                "credit_delta": entry.get("credit_delta"),
            }
        elif job:
            run.register_job(job, "artcraft_inpaint_image", artifact_base="07_inpaint_unexpected_success", timeout_seconds=900)
            dependent_jobs.append(job)

        # Paid media generation core: video, audio, mesh.
        enqueue_job_tool("artcraft_generate_video", {
            "prompt": "the small red cube rotates slowly on a white studio background",
            "model": "wan_3p0",
            "duration_seconds": 2,
            "resolution": "480p",
            "aspect_ratio": "square",
        }, "07_wan_2s_480p_cube", "wan_3p0 2s 480p retained video", timeout_seconds=1200)
        enqueue_job_tool("artcraft_generate_audio", {
            "prompt": "a single short clean electronic beep sound effect",
            "model": "suno_sounds",
            "is_loopable": False,
        }, "08_suno_short_beep", "suno_sounds retained audio (if upstream completes)", timeout_seconds=1200)
        enqueue_job_tool("artcraft_generate_3d_object", {
            "image_media_token": source_token,
            "model": "rodin_2p5_fast",
        }, "09_rodin_cube_mesh", "rodin_2p5_fast retained mesh", timeout_seconds=1200)

        # Expected upstream-500 splat and edit-video attempts.
        err, text, entry = run.call("artcraft_generate_splat", {
            "image_media_token": source_token,
            "model": "marble_0p1_mini",
            "prompt": "a minimal white studio containing the red cube",
        }, timeout=180, note="expected upstream HTTP 500 with no net spend/refund")
        job = run.extract_job(text)
        entry["job_token"] = job
        if err:
            run.upstream_failures["artcraft_generate_splat"] = {
                "expected": "upstream HTTP 500 with refund/no net deduction",
                "exact_error": redact_text(text),
                "credits_before": entry.get("credits_before"),
                "credits_after": entry.get("credits_after"),
                "credit_delta": entry.get("credit_delta"),
            }
        elif job:
            run.register_job(job, "artcraft_generate_splat", artifact_base="10_splat_unexpected_success", timeout_seconds=1200)
            dependent_jobs.append(job)

        err, text, entry = run.call("artcraft_edit_video", {
            "video_media_token": scratch_tokens.get("video", "m_missing_video"),
            "prompt": "make the test video slightly darker",
        }, timeout=180, note="expected upstream HTTP 500; scratch video input")
        job = run.extract_job(text)
        entry["job_token"] = job
        if err:
            run.upstream_failures["artcraft_edit_video"] = {
                "expected": "upstream HTTP 500",
                "exact_error": redact_text(text),
                "credits_before": entry.get("credits_before"),
                "credits_after": entry.get("credits_after"),
                "credit_delta": entry.get("credit_delta"),
            }
        elif job:
            run.register_job(job, "artcraft_edit_video", artifact_base="11_edit_video_unexpected_success", timeout_seconds=1200)
            dependent_jobs.append(job)

        # Character lifecycle evidence: create, poll, list/get/delete.
        character_name = "e2e-evidence-character-" + datetime.now(timezone.utc).strftime("%H%M%S")
        err, text, entry = run.call("artcraft_create_character", {
            "image_media_token": source_token,
            "name": character_name,
            "description": "temporary evidence-run character; deleted after verification",
            "model": "seedance_2p0",
        }, timeout=180, note="character creation evidence; character is deleted after listing")
        character_job = run.extract_job(text)
        entry["job_token"] = character_job
        if not err and character_job:
            run.register_job(character_job, "artcraft_create_character", timeout_seconds=900)
            dependent_jobs.append(character_job)
        run.evidence["character_name"] = character_name
        run.save()

        # Poll all dependent jobs. Audio is documented as stalled at 0% after ten minutes, not cancelled.
        run.poll_jobs(dependent_jobs, timeout_seconds=args.job_timeout_seconds)

        # Download every completed media-producing job output.
        for job, meta in list(run.jobs.items()):
            if job in free_jobs:
                continue
            if meta.get("result_token") and meta.get("artifact_base"):
                run.download_media(meta["result_token"], meta["artifact_base"], meta["tool"], job)

        # Character lookup and deletion after the asynchronous job completes.
        char_meta = run.jobs.get(character_job or "", {})
        if char_meta.get("outcome") == "completed":
            err, text, _ = run.call("artcraft_list_characters", {}, timeout=90, measure_credits=False, note="lookup after character job completion")
            character_token = None
            for line in (text or "").splitlines():
                if character_name in line:
                    m = CHARACTER_RE.search(line)
                    if m:
                        character_token = m.group(1)
                        break
            run.evidence["character_token_before_delete"] = character_token
            if character_token:
                run.call("artcraft_get_character", {"character_token": character_token}, timeout=90, measure_credits=False)
                run.call("artcraft_delete_character", {"character_token": character_token}, timeout=90, measure_credits=False, note="required cleanup: character deleted after evidence capture")
            else:
                run.evidence["character_delete_error"] = "character job completed but created character name was not listed"
        else:
            run.evidence["character_delete_error"] = f"character job outcome was {char_meta.get('outcome', 'missing')}; no token delete attempted"
        run.save()

        # Scratch uploads may be deleted once inpaint/edit-video probes are finished.
        for scratch in run.scratch_uploads:
            tok = scratch["media_token"]
            err, text, _ = run.call("artcraft_delete_media_file", {"media_token": tok}, timeout=90, measure_credits=False, note="delete tiny scratch upload only; generated outputs are retained")
            scratch["deleted"] = not err
            scratch["delete_text_excerpt"] = redact_text(text)[:500]
        run.save()

        # Final credit snapshot after all terminal job states we are willing to wait for.
        run.end_credits = run.credits_now()
        run.save()
        print(f"end credits: {run.end_credits}; delta: {run.start_credits - run.end_credits if run.start_credits is not None and run.end_credits is not None else '?'}")
        print(f"artifacts downloaded: {run.evidence.get('artifact_count')}")
        print(f"evidence: {out}")
        return 0
    finally:
        run.close()


if __name__ == "__main__":
    sys.exit(main())
