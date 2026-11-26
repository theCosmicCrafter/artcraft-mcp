#!/usr/bin/env python3
"""Supplemental lifecycle/read probes for the ArtCraft MCP evidence run."""
import argparse, json, re, sys, time, uuid
from datetime import datetime, timezone
from pathlib import Path
from urllib import request as urlreq, error as urlerr

sys.path.insert(0, str(Path(__file__).resolve().parent))
from artcraft_e2e_matrix import Mcp  # noqa: E402
from artcraft_retained_evidence import redact_text, first_token, MEDIA_RE, FOLDER_RE  # noqa: E402

TOKEN_LINE = re.compile(r"(?:Management token|Bookmark token|Comment token|Token)[:'\s]+([A-Za-z0-9_]{6,})", re.I)


def utcnow(): return datetime.now(timezone.utc).isoformat()

def load_cookie_header():
    root = Path.home() / "Artcraft" / "credentials"
    session = (root / "artcraft_session.txt").read_text(encoding="utf-8").strip()
    avt = (root / "artcraft_avt.txt").read_text(encoding="utf-8").strip()
    return f"visitor={avt}; session={session}"

def direct_api(method, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urlreq.Request("https://api.storyteller.ai" + path, data=data, method=method)
    req.add_header("Cookie", load_cookie_header())
    req.add_header("Accept", "application/json")
    if data is not None: req.add_header("Content-Type", "application/json")
    try:
        with urlreq.urlopen(req, timeout=60) as r:
            txt = r.read().decode("utf-8", "replace")
            return r.status, txt
    except urlerr.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace")

def jload(txt):
    try: return json.loads(txt)
    except Exception: return {"raw": txt[:500]}

class Probe:
    def __init__(self, out):
        self.mcp = Mcp(); self.out = Path(out); self.calls = {}; self.direct = []
    def save(self):
        self.out.parent.mkdir(parents=True, exist_ok=True)
        self.out.write_text(json.dumps({"schema":"artcraft-supplemental-evidence-v1","finished_at":utcnow(),"calls":self.calls,"direct_upstream":self.direct}, indent=2), encoding="utf-8")
    def call(self, tool, args=None, note=None, timeout=90):
        err, text, result = self.mcp.tool(tool, args or {}, timeout=timeout)
        e = {"at":utcnow(),"tool":tool,"arguments":args or {},"is_error":bool(err),"text":redact_text(text),"text_excerpt":redact_text(text)[:1300]}
        if note: e["note"] = note
        self.calls.setdefault(tool, []).append(e); self.save()
        return err, text, e
    def direct_call(self, label, method, path, body=None):
        status, txt = direct_api(method, path, body)
        e = {"at":utcnow(),"label":label,"method":method,"path":path,"status":status,"json":jload(txt),"body_excerpt":redact_text(txt)[:1000]}
        if body is not None: e["request_body"] = body
        self.direct.append(e); self.save()
        return status, txt, e
    def close(self): self.mcp.close()


def folder_token(text):
    m = FOLDER_RE.search(text or "")
    return m.group(1) if m else None

def line_token_for_name(text, name):
    for line in (text or "").splitlines():
        if name in line:
            return line.split("|",1)[0].strip().lstrip("- ").strip()
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--part1", default=r"C:\Users\richk\Downloads\artcraft-mcp-verification\part1_retained_evidence.json")
    ap.add_argument("--out", default=r"C:\Users\richk\Downloads\artcraft-mcp-verification\part2_supplemental_evidence.json")
    args = ap.parse_args()
    part1 = json.loads(Path(args.part1).read_text(encoding="utf-8"))
    source = (part1.get("source_image_for_edits") or {}).get("media_token") or (part1.get("retained_media_tokens") or [None])[0]
    if not source: raise SystemExit("no retained source media token in part1 evidence")
    p = Probe(args.out); ts = datetime.now(timezone.utc).strftime("%H%M%S")
    try:
        # Folder lifecycle: parent/child, rename/star/color/cover, subfolder and media membership, then delete.
        parent = child = None
        err, txt, _ = p.call("artcraft_create_folder", {"name":f"e2e-evidence-parent-{ts}","color_code":"#00aaff"}, note="supplemental lifecycle parent")
        parent = folder_token(txt)
        err, txt, _ = p.call("artcraft_create_folder", {"name":f"e2e-evidence-child-{ts}"}, note="supplemental lifecycle child")
        child = folder_token(txt)
        if parent and child:
            p.call("artcraft_get_folder", {"folder_token":parent})
            p.call("artcraft_rename_folder", {"folder_token":parent,"new_name":f"e2e-evidence-parent-renamed-{ts}"})
            p.call("artcraft_set_folder_star", {"folder_token":parent,"has_star":True})
            p.call("artcraft_set_folder_color", {"folder_token":parent,"color_code":"#ff8800"})
            p.call("artcraft_set_folder_cover_image", {"folder_token":parent,"media_file_token":source})
            p.call("artcraft_add_subfolders", {"folder_token":parent,"subfolder_tokens":[child]})
            p.call("artcraft_list_subfolders", {"folder_token":parent})
            p.call("artcraft_add_media_to_folder", {"folder_token":parent,"media_file_tokens":[source]})
            p.call("artcraft_list_folder_media_files", {"folder_token":parent})
            p.call("artcraft_move_media_to_folder", {"folder_token":child,"source_folder_token":parent,"media_file_tokens":[source]})
            p.call("artcraft_list_folder_media_files", {"folder_token":child}, note="corroborates move destination")
            p.call("artcraft_remove_media_from_folder", {"folder_token":child,"media_file_tokens":[source]})
            p.call("artcraft_remove_subfolders", {"folder_token":parent,"subfolder_tokens":[child]})
            p.call("artcraft_delete_folder", {"folder_token":child}, note="cleanup child folder")
            p.call("artcraft_delete_folder", {"folder_token":parent}, note="cleanup parent folder")
        else:
            p.calls.setdefault("folder_lifecycle_error", []).append({"error":"folder creation did not produce both tokens","parent":parent,"child":child})

        # API key lifecycle. Secret is deliberately redacted and never persisted.
        key_token = None
        err, txt, _ = p.call("artcraft_create_api_key", {"name":f"e2e-evidence-key-{ts}","description":"temporary evidence key; deleted immediately"}, note="secret redacted/discarded")
        m = re.search(r"Management token:\s*([A-Za-z0-9_]+)", txt or "")
        key_token = m.group(1) if m else None
        if key_token:
            p.call("artcraft_list_api_keys", {}, note="after create")
            p.call("artcraft_get_api_key", {"api_key_token":key_token})
            p.call("artcraft_update_api_key", {"api_key_token":key_token,"description":"updated temporary evidence key; deleting"})
            p.call("artcraft_delete_api_key", {"api_key_token":key_token}, note="required cleanup")
            p.call("artcraft_list_api_keys", {}, note="after delete; upstream lists soft-deleted rows")

        # MCP-session lifecycle through dashboard management only; never revoke the ambient credential.
        mcps_token = None; cname = f"e2e-evidence-mcp-{ts}"
        p.call("artcraft_create_mcp_session", {"client_name":cname,"client_vendor":"evidence","client_version":"1"}, note="private/refresh secrets redacted/discarded")
        err, txt, _ = p.call("artcraft_list_mcp_sessions", {}, note="find created session by client name")
        mcps_token = line_token_for_name(txt, cname)
        if mcps_token:
            p.call("artcraft_delete_mcp_session", {"session_token":mcps_token}, note="delete only the test-created session")
            p.call("artcraft_list_mcp_sessions", {}, note="after test-session delete")

        # Bookmark/comment/tag lifecycle on retained media.
        bm = cm = None
        err, txt, _ = p.call("artcraft_create_bookmark", {"media_token":source})
        m = re.search(r"Bookmark token:\s*([A-Za-z0-9_]+)", txt or ""); bm = m.group(1) if m else None
        p.call("artcraft_list_bookmarks", {}, note="after bookmark create")
        if bm: p.call("artcraft_delete_bookmark", {"bookmark_token":bm}, note="cleanup evidence bookmark")
        err, txt, _ = p.call("artcraft_create_comment", {"media_token":source,"text":"e2e evidence lifecycle comment (deleted)"})
        m = re.search(r"Comment token:\s*([A-Za-z0-9_]+)", txt or ""); cm = m.group(1) if m else None
        p.call("artcraft_list_comments", {"media_token":source}, note="after comment create")
        if cm: p.call("artcraft_delete_comment", {"comment_token":cm}, note="cleanup evidence comment")
        p.call("artcraft_list_comments", {"media_token":source}, note="after comment delete")
        p.call("artcraft_set_tags", {"media_token":source,"tags":["e2e-evidence","retained-artifact"]})
        p.call("artcraft_list_tags", {"media_token":source}, note="after tag set")
        p.call("artcraft_set_tags", {"media_token":source,"tags":[]}, note="clear evidence tags")
        p.call("artcraft_list_tags", {"media_token":source}, note="after tag clear")

        # Referral lifecycle. The MCP server exposes create/list but currently no delete tool;
        # use the upstream DELETE route only to clean up the test-created referral.
        code = ("e2e" + ts + str(int(time.time()))[-4:])[:32]
        err, txt, _ = p.call("artcraft_create_referral_code", {"code":code})
        referral_token = first_token(txt)
        p.call("artcraft_list_referral_codes", {}, note="after create")
        if not referral_token:
            # list output hides tokens, so use the upstream list JSON for cleanup correlation.
            st, body, e = p.direct_call("referral_list_for_cleanup", "GET", "/v1/user_referral_codes/list")
            for row in (e["json"].get("referral_codes") or []):
                if row.get("code") == code: referral_token = row.get("token")
        if referral_token:
            p.direct_call("referral_delete_cleanup", "DELETE", "/v1/user_referral_codes/code/" + referral_token)
            p.call("artcraft_list_referral_codes", {}, note="after direct upstream delete cleanup")

        # Checkout URL generation evidence (no payment completion).
        p.call("artcraft_create_checkout_session", {"credits_pack":"artcraft_1000"}, note="capture generated Stripe URL only; no payment")
        p.call("artcraft_create_subscription_checkout", {"plan":"artcraft_basic","cadence":"monthly"}, note="capture generated Stripe URL only; no payment")

        # Read-only expansion families and cost estimates.
        for tool, a in [
            ("artcraft_get_craft_apps_release_info",{}),
            ("artcraft_list_featured_media",{"page_size":3}),
            ("artcraft_search_featured_media",{"query":"test"}),
            ("artcraft_is_featured_item",{"entity_type":"media_file","entity_token":source}),
            ("artcraft_list_beta_keys",{"page_size":5}),
            ("artcraft_estimate_cost",{"media_type":"image","model":"flux_1_schnell","generation_mode":"text_to_image","batch_count":1}),
            ("artcraft_estimate_cost",{"media_type":"video","model":"wan_3p0","generation_mode":"text_to_video","duration_seconds":2,"resolution":"480p","batch_count":1}),
            ("artcraft_estimate_cost",{"media_type":"splat","model":"marble_0p1_mini"}),
            ("artcraft_estimate_splat_cost",{"model":"marble_0p1_mini","has_reference_image":True}),
        ]: p.call(tool,a)
        p.call("artcraft_estimate_cost", {"media_type":"audio","model":"suno_sounds"}, note="schema/upstream coverage probe: expected client-side unknown media_type")

        # Direct upstream omni cost routes are read-only estimates and cover audio/mesh,
        # which the current MCP estimator schema does not expose.
        p.direct_call("omni_cost_audio", "POST", "/v1/omni_gen/cost/audio", {"idempotency_token":str(uuid.uuid4()),"model":"suno_sounds","prompt":"short beep","is_loopable":False})
        p.direct_call("omni_cost_mesh", "POST", "/v1/omni_gen/cost/mesh", {"idempotency_token":str(uuid.uuid4()),"model":"rodin_2p5_fast","reference_image_media_tokens":[source]})
        print("supplemental evidence:", args.out)
    finally:
        p.save(); p.close()

if __name__ == "__main__": main()
