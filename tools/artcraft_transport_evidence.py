#!/usr/bin/env python3
"""Transport/protocol evidence probes for artcraft-mcp (stdio, HTTP, extensions)."""
import argparse, http.client, json, queue, re, subprocess, sys, threading, time
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from artcraft_e2e_matrix import BIN  # noqa: E402
from artcraft_retained_evidence import redact_text, sha256_file  # noqa: E402


def utcnow(): return datetime.now(timezone.utc).isoformat()

def modern_meta(files=False, tasks=True):
    ext = {"io.modelcontextprotocol/tasks": {}}
    if files: ext["io.modelcontextprotocol/files"] = {"upload": {}, "download": {}}
    return {
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name":"transport-evidence","version":"1"},
        "io.modelcontextprotocol/clientCapabilities": {"elicitation":{},"extensions":ext},
    }

def clean(obj):
    s = json.dumps(obj, ensure_ascii=False)
    s = redact_text(s).replace(str(Path.home()), "<USERPROFILE>")
    # Redact local HTTP transport session ids; they are process-local but not useful evidence.
    s = re.sub(r'("mcp-session-id"\s*:\s*")[^"]+', r'\1[redacted-local-session]', s, flags=re.I)
    try:
        return json.loads(s)
    except json.JSONDecodeError:
        # Redaction is best-effort for nested protocol transcripts; never drop
        # the underlying message merely because a text regex hit schema prose.
        return obj

class Stdio:
    def __init__(self, transcript, label, extra_args=None):
        self.p = subprocess.Popen([str(BIN)] + list(extra_args or []), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, encoding="utf-8", errors="replace")
        self.q = queue.Queue(); self.next = 0; self.transcript=transcript; self.label=label
        threading.Thread(target=self.reader, daemon=True).start()
    def reader(self):
        for line in self.p.stdout:
            line=line.strip()
            if line.startswith('{'):
                try: self.q.put(json.loads(line))
                except Exception: pass
        self.q.put(None)
    def send(self, method, params=None, note=None, timeout=60):
        self.next += 1; rid=self.next
        req={"jsonrpc":"2.0","id":rid,"method":method,"params":params or {}}
        self.transcript.append({"at":utcnow(),"channel":self.label,"direction":"client->server","note":note,"message":clean(req)})
        self.p.stdin.write(json.dumps(req)+"\n"); self.p.stdin.flush()
        end=time.time()+timeout
        while time.time()<end:
            try: msg=self.q.get(timeout=max(1,end-time.time()))
            except queue.Empty: raise TimeoutError(method)
            self.transcript.append({"at":utcnow(),"channel":self.label,"direction":"server->client","note":note,"message":clean(msg)})
            if msg is None: raise RuntimeError("server EOF")
            if msg.get("id")==rid: return msg
        raise TimeoutError(method)
    def notify(self, method, params=None):
        msg={"jsonrpc":"2.0","method":method,"params":params or {}}
        self.transcript.append({"at":utcnow(),"channel":self.label,"direction":"client->server","message":clean(msg)})
        self.p.stdin.write(json.dumps(msg)+"\n"); self.p.stdin.flush()
    def close(self):
        try: self.p.kill()
        except Exception: pass

def parse_sse(body):
    out=[]
    for line in body.splitlines():
        if line.startswith('data: '):
            try: out.append(json.loads(line[6:]))
            except Exception: pass
    return out

def http_json(host, port, method, path, obj=None, headers=None):
    conn=http.client.HTTPConnection(host,port,timeout=30)
    body=None if obj is None else json.dumps(obj)
    h={"Accept":"application/json, text/event-stream","Content-Type":"application/json"}
    if headers: h.update(headers)
    conn.request(method,path,body=body,headers=h)
    r=conn.getresponse(); data=r.read().decode('utf-8','replace'); hh=dict((k.lower(),v) for k,v in r.getheaders()); conn.close()
    messages=[]
    if data.strip().startswith('{'):
        try: messages=[json.loads(data)]
        except Exception: messages=[]
    else: messages=parse_sse(data)
    return {"status":r.status,"headers":hh,"body_excerpt":redact_text(data)[:1500],"messages":clean(messages)}

def wait_health(port, proc, timeout=10):
    end=time.time()+timeout
    while time.time()<end:
        if proc.poll() is not None: raise RuntimeError(f"HTTP server exited {proc.returncode}")
        try:
            conn=http.client.HTTPConnection('127.0.0.1',port,timeout=2); conn.request('GET','/health'); r=conn.getresponse(); body=r.read().decode(); conn.close()
            if r.status==200 and body=='OK': return {"status":r.status,"body":body}
        except Exception: pass
        time.sleep(.25)
    raise TimeoutError('health endpoint did not become ready')

def main():
    ap=argparse.ArgumentParser(); ap.add_argument('--out',default=r'C:\Users\richk\Downloads\artcraft-mcp-verification\part3_transport_evidence.json'); ap.add_argument('--verification-dir',default=r'C:\Users\richk\Downloads\artcraft-mcp-verification'); ap.add_argument('--port',type=int,default=8891); ap.add_argument('--task-timeout',type=int,default=600)
    args=ap.parse_args(); transcript=[]; evidence={"schema":"artcraft-transport-evidence-v1","started_at":utcnow(),"binary":str(BIN).replace(str(Path.home()),'<USERPROFILE>'),"transcript":transcript}
    vdir=Path(args.verification_dir); vdir.mkdir(parents=True,exist_ok=True)

    # Legacy stdio handshake and full catalog.
    st=Stdio(transcript,'stdio-legacy')
    init=st.send('initialize',{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"transport-evidence","version":"1"}},note='legacy initialize')
    st.notify('notifications/initialized',{})
    tools=st.send('tools/list',{},note='legacy tools/list')
    names=[t.get('name') for t in tools.get('result',{}).get('tools',[])]
    evidence['stdio_legacy']={"initialize_result":clean(init.get('result',{})),"tool_count":len(names),"first_tools":names[:8],"has_99_tools":len(names)==99}
    # Apps resources evidence on this legacy session.
    rl=st.send('resources/list',{},note='apps resource list')
    rr=st.send('resources/read',{"uri":"ui://artcraft/credit-dashboard"},note='apps resource read')
    resources=rl.get('result',{}).get('resources',[])
    html=''.join(c.get('text','') for c in rr.get('result',{}).get('contents',[]))
    evidence['apps_extension']={"resource_count":len(resources),"uris":[r.get('uri') for r in resources],"credit_dashboard_mime":(rr.get('result',{}).get('contents') or [{}])[0].get('mimeType'),"html_excerpt":html[:240],"csp_meta_present":bool(resources and resources[0].get('_meta',{}).get('ui',{}).get('csp'))}
    st.close()

    # Modern progressive catalog, MRTR accept, tasks extension, and retained free image.
    meta=modern_meta(files=True,tasks=True); sm=Stdio(transcript,'stdio-modern-2026-07-28')
    tl=sm.send('tools/list',{"_meta":meta},note='modern progressive tools/list')
    mnames=[t.get('name') for t in tl.get('result',{}).get('tools',[])]
    callargs={"prompt":"a small violet pyramid on a white background, protocol evidence test image","model":"flux_1_schnell","image_batch_count":1,"aspect_ratio":"square"}
    r1=sm.send('tools/call',{"name":"artcraft_generate_image","arguments":callargs,"_meta":meta},note='MRTR round 1: billable tool returns input_required')
    res1=r1.get('result',{}); state=res1.get('requestState')
    evidence['modern_progressive']={"tool_count":len(mnames),"tool_names":mnames,"ttlMs":tl.get('result',{}).get('_meta',{}).get('ttlMs')}
    evidence['mrtr_round1']={"resultType":res1.get('resultType'),"has_requestState":bool(state),"input_request_keys":list((res1.get('inputRequests') or {}).keys()),"elicitation_message":(((res1.get('inputRequests') or {}).get('confirm') or {}).get('message') or '')}
    taskid=None; mrtr_media=None; task_final=None
    if state:
        r2=sm.send('tools/call',{"name":"artcraft_generate_image","arguments":callargs,"requestState":state,"inputResponses":{"confirm":{"action":"accept","content":{"confirm":True}}},"_meta":meta},note='MRTR round 2: accept free flux generation',timeout=90)
        res2=r2.get('result',{}); taskid=res2.get('taskId')
        evidence['mrtr_round2_accept']={"resultType":res2.get('resultType'),"taskId":taskid,"status":res2.get('status'),"raw_excerpt":redact_text(json.dumps(res2))[:700]}
        start=time.time(); polls=[]
        while taskid and time.time()-start<args.task_timeout:
            tg=sm.send('tasks/get',{"taskId":taskid,"_meta":meta},note='poll tasks/get',timeout=60)
            tr=tg.get('result',{}); status=tr.get('status'); polls.append({"status":status,"at":utcnow()})
            if status in ('completed','failed','cancelled'):
                task_final=tr; break
            time.sleep(8)
        evidence['tasks_extension']={"task_created":bool(taskid),"polls":polls,"final_status":(task_final or {}).get('status'),"final_excerpt":redact_text(json.dumps(task_final or {}))[:1500]}
        txt=json.dumps(task_final or {})
        mm=re.search(r'\b(m_[A-Za-z0-9]{6,})\b',txt); mrtr_media=mm.group(1) if mm else None
    # Download the free MRTR image if the task completed; it is retained just like Part 1 outputs.
    if mrtr_media:
        dl=sm.send('tools/call',{"name":"artcraft_download_media_file","arguments":{"media_token":mrtr_media,"download_dir":str(vdir),"filename":"10_mrtr_flux_protocol_image.jpg"},"_meta":meta},note='retain MRTR free image output',timeout=180)
        fp=vdir/'10_mrtr_flux_protocol_image.jpg'
        evidence['mrtr_media_artifact']={"media_token":mrtr_media,"filename":fp.name,"downloaded":fp.exists() and fp.stat().st_size>0,"size_bytes":fp.stat().st_size if fp.exists() else 0,"sha256":sha256_file(fp) if fp.exists() else None,"download_result_excerpt":redact_text(json.dumps(dl.get('result',{})))[:800]}

    # Files extension upload-target negotiation only (no bytes uploaded, no retry).
    png_size=68
    fu=sm.send('tools/call',{"name":"artcraft_upload_image","arguments":{"file_path":{"id":"file-evidence-1","mimeType":"image/png","size":png_size,"name":"evidence.png"}},"_meta":meta},note='files extension negotiation only')
    fres=fu.get('result',{}); uploads=(fres.get('_meta') or {}).get('io.modelcontextprotocol/files',{}).get('uploads') or []
    evidence['files_extension']={"resultType":fres.get('resultType'),"has_requestState":bool(fres.get('requestState')),"upload_count":len(uploads),"upload_targets":clean(uploads),"negotiation_only":True}
    sm.close()

    # HTTP transport: health, legacy initialize/session tools call, hostile Host rejection.
    proc=subprocess.Popen([str(BIN),'--transport','http','--bind',f'127.0.0.1:{args.port}'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        health=wait_health(args.port,proc)
        initreq={"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"http-evidence","version":"1"}}}
        hi=http_json('127.0.0.1',args.port,'POST','/mcp',initreq)
        sid=(hi['headers'].get('mcp-session-id') or '')
        notif=http_json('127.0.0.1',args.port,'POST','/mcp',{"jsonrpc":"2.0","method":"notifications/initialized","params":{}},headers={'Mcp-Session-Id':sid} if sid else None)
        tc=http_json('127.0.0.1',args.port,'POST','/mcp',{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"artcraft_get_credits","arguments":{}}},headers={'Mcp-Session-Id':sid} if sid else None)
        hostile=http_json('127.0.0.1',args.port,'POST','/mcp',initreq,headers={'Host':'evil.example'})
        evidence['http_transport']={"bind":f'127.0.0.1:{args.port}',"health":health,"initialize_status":hi['status'],"initialize_messages":hi['messages'],"session_id_present":bool(sid),"initialized_notification_status":notif['status'],"tools_call_status":tc['status'],"tools_call_messages":tc['messages'],"hostile_host_status":hostile['status'],"hostile_host_body":hostile['body_excerpt']}
    finally:
        proc.terminate()
        try: proc.wait(timeout=5)
        except Exception: proc.kill()

    evidence['finished_at']=utcnow()
    out=Path(args.out); out.parent.mkdir(parents=True,exist_ok=True); out.write_text(json.dumps(evidence,indent=2,ensure_ascii=False),encoding='utf-8')
    print('transport evidence:',out)

if __name__=='__main__': main()
