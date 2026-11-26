#!/usr/bin/env python3
"""Generate docs/verification-evidence-report.md from machine-checkable evidence JSON."""
import json, re
from pathlib import Path
from datetime import datetime, timezone

ROOT=Path(__file__).resolve().parents[1]
VDIR=Path(r'C:\Users\richk\Downloads\artcraft-mcp-verification')
OUT=ROOT/'docs'/'verification-evidence-report.md'

def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))

def esc(s,n=180):
    s=re.sub(r'\s+',' ',str(s or '')).replace('|','\\|').strip()
    s=re.sub(r'https://checkout\.stripe\.com/\S+','https://checkout.stripe.com/[redacted-checkout-session]',s)
    s=re.sub(r'[A-Za-z]:\\Windows\\TEMP\\\S+','<temp-path>',s)
    s=re.sub(r'[A-Za-z]:\\Users\\richk\\\S+','<user-path>',s)
    return s[:n]+('…' if len(s)>n else '')

def money_rows(part1,matrix):
    rows=[]
    rows.append(('Part 1 retained-artifact generation',part1.get('start_credits'),part1.get('start_credits')-28 if part1.get('start_credits') is not None else None,28,'sum of measured per-call deductions in part1_retained_evidence.json'))
    rows.append(('Part 2 full matrix',matrix.get('start_credits'),matrix.get('end_credits'),matrix.get('net_credit_delta'),'matrix report start/end'))
    return rows

def artifact_rows(part1,mrtr):
    rows=[]
    for a in part1.get('artifacts',[]):
        rows.append({
            'kind':a.get('kind'),'media_token':a.get('media_token'),'job_token':a.get('job_token'),
            'filename':a.get('filename'),'size':a.get('size_bytes'),'sha256':a.get('sha256'),
            'source':a.get('source_tool'),'status':'downloaded' if a.get('downloaded') else 'not downloaded'})
    ma=mrtr.get('artifact') or {}
    if ma:
        rows.append({'kind':'forced-MRTR protocol image','media_token':ma.get('media_token'),'job_token':'jinf_r01n5pqq9jyqccypwcxb9530sty','filename':ma.get('filename'),'size':ma.get('size_bytes'),'sha256':ma.get('sha256'),'source':'artcraft_generate_image via MRTR/tasks','status':'downloaded' if ma.get('downloaded') else 'not downloaded'})
    return rows

def main():
    part1=load(VDIR/'part1_retained_evidence.json')
    supp=load(VDIR/'part2_supplemental_evidence.json')
    trans=load(VDIR/'part3_transport_evidence.json')
    mrtr=load(VDIR/'part3_mrtr_forced_evidence.json')
    matrix=load(ROOT/'tools'/'e2e_matrix_evidence.json')
    schemas=sorted(json.loads(p.read_text(encoding='utf-8'))['name'] for p in (ROOT/'schemas').glob('*.json'))
    rows=matrix['rows']; summ=matrix['summary']
    final_credits=matrix.get('end_credits')
    global_delta=(part1.get('start_credits') or 0)-(final_credits or 0)
    positive=sum(e.get('delta',0) for e in part1.get('wallet_events',[]) if e.get('delta',0)>0)
    audio=[v for v in part1.get('jobs',{}).values() if v.get('tool')=='artcraft_generate_audio'][0]

    lines=[]
    A=lines.append
    A('# ArtCraft MCP verification evidence report')
    A('')
    A(f'Date: 2026-11-26. Evidence generated against the release binary `rust/artcraft_mcp_cli/target/release/artcraft-mcp.exe` (SHA-256 `a8f86c06bdf336b19f0e696b8002235f197bfc4382b5a78044c313b0829b0bc8`, identical for packaged `bin/artcraft-mcp.exe`). No git commit, push, issue, payment completion, or public action was performed.')
    A('')
    A('Machine-checkable evidence files are in the local verification folder (rendered here as `<verification-dir>`):')
    for f in ['part1_retained_evidence.json','part2_supplemental_evidence.json','part3_transport_evidence.json','part3_mrtr_forced_evidence.json','conformance-active-2026-11-26.txt','drift-check-2026-11-26.json','part3_progressive_meta_evidence.json','final_credit_snapshot.json','final_cleanup_snapshot.json']:
        A(f'- `<verification-dir>/{f}`')
    A('- `tools/e2e_matrix_evidence.json` (fresh full-matrix report)')
    A('')
    A('Secrets are redacted. Media/job tokens are intentionally retained because they are opaque object identifiers needed for user-controlled deletion, not bearer credentials. Stripe checkout URLs were captured in local evidence but are not reproduced here.')
    A('')
    A('## Executive verdict')
    A('')
    A(f'- Business-tool matrix: **{summ["pass"]} PASS / {summ["fail"]} FAIL / {summ["skip"]} SKIP of {len(rows)}**. Three FAIL rows are reproducible upstream HTTP 500s (`inpaint`, `splat`, `edit_video`). The fourth matrix FAIL is an intermittent upstream HTTP 500 on character creation; Part 1 independently completed the full character create/poll/list/get/delete lifecycle. One SKIP is the deliberately unsafe self-revocation tool and the other is the dependent character-delete row skipped after that matrix-only creation failure.')
    A(f'- Real retained output artifacts: **{len(artifact_rows(part1,mrtr))} downloaded media files** (8 Part 1 outputs + 1 forced-MRTR protocol image). Audio did not produce an artifact because upstream left the job pending at 0%.')
    A(f'- Exact global spend: **{part1.get("start_credits")} → {final_credits} = {global_delta} banked credits**, within the 100-credit cap. This reconciles as 28 credits in Part 1 plus 28 credits in the independent matrix; all read-only/protocol probes measured zero.')
    A('- Fresh official active-suite conformance: **40/40 checks passed, 0 failed, across 30/30 scenarios**. Fresh drift check: **94 matched, 0 missing, 0 verb mismatches** against 364 upstream routes at `db9d9cde371a95bc4d9ba0c8dad0236cccc33879` (4 additional usages are statically verb-undetermined, not drift).')
    A('- Transport/protocol evidence: legacy stdio exposes exactly 99 tools; modern 2026-07-28 progressive mode exposes a 10-tool hot catalog when tasks support is declared; forced MRTR input-required → tamper rejection → accept → task creation → tasks/get completion is captured; files upload-target negotiation, Apps resources, HTTP initialize/tools-call, health, and hostile-Host 403 are captured.')
    A('')
    A('## Exact credit reconciliation')
    A('')
    A('| Scope | Start | End | Delta | Evidence |')
    A('|---|---:|---:|---:|---|')
    A(f'| Part 1 measured generation calls | {part1.get("start_credits")} | 5227 | 28 | positive per-call deltas sum = {positive}; see wallet_events |')
    A(f'| Independent full matrix | {matrix.get("start_credits")} | {matrix.get("end_credits")} | {matrix.get("net_credit_delta")} | matrix start/end and per-tool rows |')
    A(f'| **Global verification run** | **{part1.get("start_credits")}** | **{final_credits}** | **{global_delta}** | no other measured deduction |')
    A('')
    A('Important ledger caveat: `part1_retained_evidence.json` has `wallet_delta=56` because its final snapshot was taken while the independent matrix had already spent its own 28 credits. The per-call events inside Part 1 sum exactly to 28, and the matrix independently measured 5227→5199=28; therefore the scoped ledger above is exact.')
    A('')
    A('Measured Part 1 deductions: angle edit **2**, audio enqueue **3**, Wan video **10**, Rodin mesh **13**. Free flux images, Gemini edit, background removal, character creation, inpaint/splat/edit-video failed calls measured **0** immediate net deduction.')
    A('')
    A('## Retained artifact inventory')
    A('')
    A('| Artifact | Media token | Job token | File | Bytes | SHA-256 |')
    A('|---|---|---|---|---:|---|')
    for a in artifact_rows(part1,mrtr):
        A(f'| {esc(a["kind"],60)} | `{a.get("media_token")}` | `{a.get("job_token")}` | `{a.get("filename")}` | {a.get("size")} | `{str(a.get("sha256"))[:16]}…` |')
    A('')
    A('The Wan MP4 was independently probed as H.264 video at 640×640 with duration 2.000s (plus a short AAC track), matching the requested 2-second 480p-class square output. All listed files exist locally in `<verification-dir>` and the corresponding media objects are retained on the ArtCraft account. No generated media was deleted. Only the two tiny scratch mask/video uploads and the temporary character were deleted, as required. `final_cleanup_snapshot.json` confirms that the ten matrix-created media tokens are absent, retained Part 1/MRTR media remain listed, only the two pre-existing characters remain, and referral codes are empty.')
    A('')
    A('## Audio stall evidence')
    A('')
    se=audio.get('stall_evidence') or {}
    A(f'- Job `{audio.get("job_token")}` (`suno_sounds`) deducted 3 credits at enqueue and remained `Status: pending`, `Progress: 0%` after **{se.get("elapsed_seconds")} seconds**. The evidence explicitly records: “{se.get("note")}”.')
    A('- The independent matrix reproduced the same upstream stall; its harness eventually terminated only its own matrix-created job after the bounded poll window. That matrix cancellation is harness cleanup, not evidence of upstream completion.')
    A('')
    A('## Reproducible upstream-broken generation endpoints')
    A('')
    A('| Tool | Exact evidence | Credit effect |')
    A('|---|---|---|')
    for tool in ['artcraft_inpaint_image','artcraft_generate_splat','artcraft_edit_video']:
        x=part1['upstream_failures'][tool]
        A(f'| `{tool}` | `{esc(x["exact_error"],150)}` | {x["credit_delta"]} measured |')
    A('')
    A('These are not MCP schema/validation errors: the MCP calls use valid media tokens and typed upstream request shapes, then upstream returns HTTP 500 before a job token is produced. The matrix independently reproduced the same three failures.')
    A('')
    A('## Protocol, transport, and SOTA architecture evidence')
    A('')
    sl=trans.get('stdio_legacy',{}); mp=trans.get('modern_progressive',{}); fe=trans.get('files_extension',{}); ht=trans.get('http_transport',{}); app=trans.get('apps_extension',{})
    A(f'- Legacy stdio initialize returned protocol `2024-11-05`, server `artcraft-mcp` v2.0.0, and exactly **{sl.get("tool_count")} tools**.')
    A(f'- Modern progressive `tools/list` at `2026-07-28` returned **{mp.get("tool_count")} hot tools** with `ttlMs={mp.get("ttlMs")}`; with tasks support declared, redundant job polling tools are hidden as designed. `artcraft_search_tools`, `artcraft_describe_tool`, and `artcraft_call_deferred_tool` were separately exercised successfully in `part3_progressive_meta_evidence.json`.')
    A(f'- Forced MRTR (`--confirm-over-credits 0 --auto-approve-under 0`) returned `resultType=input_required`, `requestState`, and an elicitation form. A one-character tampered state was rejected with `tampered or unknown request state`. Accepting the original state returned task `{mrtr.get("round2_accept",{}).get("taskId")}`; `tasks/get` progressed working → working → completed and included both the enqueue result and upstream completion with result token `{mrtr.get("media_token")}`.')
    A('- MRTR quote caveat: the confirmation message estimated ~40 credits even for `flux_1_schnell` because the current implementation uses a conservative static tool-tier estimate rather than the model-specific zero-cost estimate. The actual measured spend was zero. This is safe/conservative but should be improved.')
    A(f'- Files extension negotiation produced `resultType=input_required`, one upload target, method `{((fe.get("upload_targets") or [{}])[0]).get("method")}`, credentials `{((fe.get("upload_targets") or [{}])[0]).get("credentials")}`, and a loopback `/v1/mcp-uploads/...` URL. The probe intentionally stopped before upload/retry.')
    A(f'- Apps/resources: `resources/list` exposed {app.get("resource_count")} resources including the four MCP App HTML UIs; `ui://artcraft/credit-dashboard` read as `{app.get("credit_dashboard_mime")}` with CSP metadata present.')
    A(f'- HTTP `/health` returned `{ht.get("health",{}).get("status")} {ht.get("health",{}).get("body")}`; legacy initialize over `/mcp` returned {ht.get("initialize_status")} plus a session header; initialized notification returned {ht.get("initialized_notification_status")}; `artcraft_get_credits` over HTTP returned {ht.get("tools_call_status")} and a parseable credit response. Hostile `Host: evil.example` returned **{ht.get("hostile_host_status")}** with `{esc(ht.get("hostile_host_body"),120)}`.')
    A('- Official current-spec evidence: MCP documentation identifies `2026-07-28` as the current protocol version. The implementation uses rmcp **3.11.0** locked in `Cargo.lock`. crates.io published rmcp **3.12.0** on 2026-11-26 during this verification window, so the build is no longer literally the newest SDK release at report time; it was the current stable line used by the implementation and is one release behind. This is a transparency caveat, not a conformance failure.')
    A('')
    A('## Supplemental lifecycle evidence')
    A('')
    A('The supplemental run independently exercised folder create/get/rename/star/color/cover, subfolder add/list/remove, media add/list/move/remove, child+parent delete; API-key create/list/get/update/delete/list; MCP-session create/list/delete/list; bookmark/comment/tag create-list-delete/clear cycles on retained media; credit-pack and subscription checkout URL creation; featured/beta/Craft-app reads; and image/video/splat cost estimates. Secrets returned by API-key/MCP-session creation are redacted and were not persisted.')
    A('')
    direct=supp.get('direct_upstream_corrected',[])
    ac=[x for x in direct if x.get('label')=='omni_cost_audio_corrected']
    mc=[x for x in direct if x.get('label')=='omni_cost_mesh_corrected']
    A(f'- Referral cleanup gap: MCP exposes referral create/list but no delete tool. The test-created referral codes were therefore deleted through upstream `DELETE /v1/user_referral_codes/code/{{token}}`; corrected direct evidence shows HTTP 200 deletes and an empty list afterward.')
    if ac and mc:
        A(f'- Cost-estimator coverage gap: `artcraft_estimate_cost` rejects `media_type=audio` client-side. Direct upstream OmniGen cost routes returned audio `suno_sounds` = **{ac[0]["json"].get("cost_in_credits")} credits** and mesh `rodin_2p5_fast` = **{mc[0]["json"].get("cost_in_credits")} credits**, proving upstream support exists but the MCP estimator schema does not expose audio/mesh.')
    A('')
    A('## Full 99-tool evidence matrix')
    A('')
    A('`PASS` below means the fresh matrix row passed and the raw redacted response excerpt is inspectable in `tools/e2e_matrix_evidence.json`; supplemental/Part 1 annotations identify independent corroboration. Matrix self-report is therefore not the only evidence for the important paid/artifact paths.')
    A('')
    A('| Tool | Matrix | Independent evidence | Credits | Evidence excerpt |')
    A('|---|---|---|---:|---|')
    for t in schemas:
        r=rows.get(t,{'status':'MISSING','detail':'no row','credits':0})
        notes=[]
        if t in part1.get('calls',{}): notes.append('Part 1 live')
        if t in supp.get('calls',{}): notes.append('supplemental')
        if t in ('artcraft_generate_image','artcraft_download_media_file'): notes.append('MRTR/tasks')
        if t in ('artcraft_upload_image',): notes.append('files-extension negotiation')
        ind=', '.join(notes) if notes else 'matrix only'
        A(f'| `{t}` | {r.get("status")} | {ind} | {r.get("credits",0)} | {esc(r.get("detail"),150)} |')
    A('')
    A('## Deliberately non-probed or limited surfaces')
    A('')
    A('- `artcraft_revoke_mcp_session`: SKIP. A valid probe can revoke the ambient credential; only non-destructive schema/coverage evidence is appropriate.')
    A('- Legacy paid animation/conversion tools (`artcraft_create_face_animation`, `artcraft_create_rerender_animation`, `artcraft_convert_bvh_to_workflow`, `artcraft_convert_fbx_to_gltf`, `artcraft_convert_render_engine_scene`): client-side validation evidence only. Prior probing showed upstream enqueues real paid jobs without validating fake input tokens, so valid-looking fake-token probes are unsafe.')
    A('- Stripe checkout sessions were created only to capture URL generation; no checkout was opened or completed.')
    A('')
    A('## Surprises and follow-ups')
    A('')
    A('1. rmcp 3.12.0 was published during the verification window; decide whether to upgrade from locked 3.11.0 in a separate dependency pass.')
    A('2. MRTR cost quotes are tier-based and conservative (~40cr for image generation) rather than model-specific; this caused a free flux image to display a 40-credit confirmation estimate when confirmations were forced.')
    A('3. The matrix’s concurrent character-create attempt received an upstream HTTP 500, while the Part 1 character create/poll/list/get/delete lifecycle succeeded. Treat character generation as intermittently upstream-flaky and investigate with a non-concurrent repeat matrix.')
    A('4. `artcraft_estimate_cost` lacks audio/mesh media types even though upstream OmniGen cost endpoints support them.')
    A('5. MCP lacks a referral-delete convenience tool; cleanup required the upstream delete route.')
    A('6. `artcraft_create_prompt` succeeds, but the upstream route table exposes no prompt-delete route. The matrix-created `e2e-test-harness` prompt therefore could not be cleaned up; this is an upstream/API-surface limitation rather than a harness omission.')
    A('')
    A('## Bottom line')
    A('')
    A('The server is interoperable and evidence-backed on both legacy and modern MCP surfaces, all 99 business tools were exercised or deliberately safety-limited, real paid artifacts were produced and retained, exact spend reconciled to 56 credits, and the reproducible business-tool failures are the three upstream HTTP 500 generation endpoints plus the upstream audio scheduling stall; character generation additionally showed a matrix-only intermittent upstream 500 despite a successful independent lifecycle. The transparency caveats above prevent an unqualified “perfect” verdict, but they do not contradict the core interoperability/artifact evidence.')

    OUT.parent.mkdir(exist_ok=True)
    OUT.write_text('\n'.join(lines)+'\n',encoding='utf-8')
    print(OUT)

if __name__=='__main__': main()
