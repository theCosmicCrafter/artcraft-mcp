#!/usr/bin/env python3
"""Pre-release polish evidence: model-specific MRTR quotes + audio/mesh cost estimates.

ZERO SPEND: every MRTR confirmation is declined; cost-estimate calls are
read-only. The optional free-tier execution check uses flux_1_schnell
(free+unlimited) and deletes the produced media file.
"""
import json, re, sys, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from artcraft_transport_evidence import Stdio, modern_meta, clean, utcnow  # noqa: E402
from artcraft_retained_evidence import redact_text  # noqa: E402


def elicit_message(result):
    ir = (result or {}).get("inputRequests") or {}
    confirm = ir.get("confirm") or {}
    return (confirm.get("params") or {}).get("message") or confirm.get("message") or ""


def roundtrip_decline(st, name, args, meta, transcript_note):
    """Fire a tools/call, capture the MRTR elicitation, then decline it."""
    r1 = st.send("tools/call", {"name": name, "arguments": args, "_meta": meta},
                 note=f"{transcript_note} round 1", timeout=90)
    res1 = r1.get("result", {})
    out = {
        "resultType": res1.get("resultType"),
        "elicitation_message": elicit_message(res1),
        "has_requestState": bool(res1.get("requestState")),
    }
    state = res1.get("requestState")
    if state:
        r2 = st.send("tools/call", {
            "name": name, "arguments": args, "requestState": state,
            "inputResponses": {"confirm": {"action": "decline"}},
            "_meta": meta,
        }, note=f"{transcript_note} decline round 2", timeout=60)
        out["decline_excerpt"] = redact_text(json.dumps(r2.get("result", {})))[:400]
    return out


def main():
    out = Path(r"C:\Users\richk\Downloads\artcraft-mcp-verification\polish_quote_evidence.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    transcript = []
    meta = modern_meta(files=False, tasks=True)
    evidence = {"schema": "artcraft-polish-quote-evidence-v1", "started_at": utcnow(),
                "spend": "zero — all confirmations declined; estimates are read-only",
                "transcript": transcript}

    # --- Forced-MRTR server: quote accuracy under confirm-everything mode ---
    st = Stdio(transcript, "stdio-forced-mrtr",
               extra_args=["--confirm-over-credits", "0", "--auto-approve-under", "0"])
    try:
        evidence["flux_schnell_quote"] = roundtrip_decline(
            st, "artcraft_generate_image",
            {"prompt": "quote probe — will be declined", "model": "flux_1_schnell",
             "image_batch_count": 1, "aspect_ratio": "square"},
            meta, "free schnell quote")
        evidence["flux_dev_quote"] = roundtrip_decline(
            st, "artcraft_generate_image",
            {"prompt": "quote probe — will be declined", "model": "flux_1_dev",
             "image_batch_count": 1, "aspect_ratio": "square"},
            meta, "paid dev quote")
        # Tier-fallback path: unparseable model -> conservative magnitude, labeled approximate.
        evidence["fallback_quote"] = roundtrip_decline(
            st, "artcraft_generate_video",
            {"prompt": "quote probe — will be declined", "model": "seedance_2_fast"},
            meta, "fallback tier quote")
        # Audio + mesh cost estimates: read-only upstream cost routes.
        for mt, model in (("audio", "suno_sounds"), ("mesh", "rodin_2p5_fast")):
            r = st.send("tools/call", {
                "name": "artcraft_estimate_cost",
                "arguments": {"media_type": mt, "model": model}, "_meta": meta},
                note=f"estimate_cost {mt}/{model}", timeout=60)
            evidence[f"estimate_{mt}"] = {
                "isError": r.get("result", {}).get("isError"),
                "text": redact_text(json.dumps(
                    [c.get("text", "") for c in r.get("result", {}).get("content", [])]))[:600],
            }
    finally:
        st.close()

    # --- Default-threshold server: free schnell must NOT gate; runs free ---
    st2 = Stdio(transcript, "stdio-default-threshold", extra_args=[])
    try:
        args = {"prompt": "a tiny gray pebble on a white background, quote evidence image",
                "model": "flux_1_schnell", "image_batch_count": 1, "aspect_ratio": "square"}
        r1 = st2.send("tools/call", {"name": "artcraft_generate_image",
                                     "arguments": args, "_meta": meta},
                      note="default threshold: free model bypasses MRTR", timeout=90)
        res1 = r1.get("result", {})
        evidence["default_threshold_free"] = {
            "resultType": res1.get("resultType"),
            "bypassed_mrtr": res1.get("resultType") != "input_required",
            "taskId": res1.get("taskId"),
        }
        taskid = res1.get("taskId")
        polls, final, start = [], {}, time.time()
        while taskid and time.time() - start < 240:
            tg = st2.send("tasks/get", {"taskId": taskid, "_meta": meta},
                          note="poll free schnell task", timeout=60)
            final = tg.get("result", {})
            polls.append({"at": utcnow(), "status": final.get("status")})
            if final.get("status") in ("completed", "failed", "cancelled"):
                break
            time.sleep(6)
        evidence["default_threshold_free"]["task_polls"] = polls
        evidence["default_threshold_free"]["final_status"] = final.get("status")
        mm = re.search(r"\b(m_[A-Za-z0-9]{6,})\b", json.dumps(final or {}))
        media = mm.group(1) if mm else None
        evidence["default_threshold_free"]["media_token"] = media
        if media:
            dl = st2.send("tools/call", {
                "name": "artcraft_delete_media_file",
                "arguments": {"media_token": media}, "_meta": meta},
                note="cleanup: delete free evidence image", timeout=60)
            evidence["default_threshold_free"]["cleanup"] = redact_text(
                json.dumps(dl.get("result", {})))[:400]
    finally:
        st2.close()

    # --- Legacy handshake smoke: 99 tools ---
    st3 = Stdio(transcript, "stdio-legacy")
    try:
        init = st3.send("initialize", {
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "polish-evidence", "version": "1"}}, note="legacy initialize")
        st3.notify("notifications/initialized", {})
        tl = st3.send("tools/list", {}, note="legacy tools/list")
        names = [t.get("name") for t in tl.get("result", {}).get("tools", [])]
        evidence["legacy_smoke"] = {
            "server": (init.get("result", {}).get("serverInfo") or {}).get("name"),
            "version": (init.get("result", {}).get("serverInfo") or {}).get("version"),
            "protocol": init.get("result", {}).get("protocolVersion"),
            "tool_count": len(names),
        }
    finally:
        st3.close()

    evidence["finished_at"] = utcnow()
    out.write_text(json.dumps(evidence, indent=2, ensure_ascii=False), encoding="utf-8")
    print("polish quote evidence:", out)
    print(json.dumps({k: v for k, v in evidence.items() if k != "transcript"}, indent=2)[:4000])


if __name__ == "__main__":
    main()
