#!/usr/bin/env python3
"""Focused forced-MRTR accept/tasks evidence probe."""
import json,re,sys,time
from datetime import datetime, timezone
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent))
from artcraft_transport_evidence import Stdio, modern_meta, clean, utcnow
from artcraft_retained_evidence import redact_text, sha256_file


def main():
    out=Path(r'C:\Users\richk\Downloads\artcraft-mcp-verification\part3_mrtr_forced_evidence.json')
    vdir=Path(r'C:\Users\richk\Downloads\artcraft-mcp-verification')
    transcript=[]; meta=modern_meta(files=True,tasks=True)
    st=Stdio(transcript,'stdio-modern-forced-mrtr',extra_args=['--confirm-over-credits','0','--auto-approve-under','0'])
    evidence={'schema':'artcraft-forced-mrtr-evidence-v1','started_at':utcnow(),'server_args':['--confirm-over-credits','0','--auto-approve-under','0'],'transcript':transcript}
    try:
        args={"prompt":"a tiny orange cone on a white background, forced MRTR evidence image","model":"flux_1_schnell","image_batch_count":1,"aspect_ratio":"square"}
        r1=st.send('tools/call',{"name":"artcraft_generate_image","arguments":args,"_meta":meta},note='forced MRTR round 1',timeout=60)
        res1=r1.get('result',{}); state=res1.get('requestState')
        evidence['round1']={"resultType":res1.get('resultType'),"has_requestState":bool(state),"input_request_keys":list((res1.get('inputRequests') or {}).keys()),"raw_excerpt":redact_text(json.dumps(res1))[:1300]}
        if not state: raise RuntimeError('forced MRTR did not return requestState')
        # Tampered state must be rejected; this probe consumes nothing and creates no job.
        bad=state[:-1]+('A' if state[-1]!='A' else 'B')
        rt=st.send('tools/call',{"name":"artcraft_generate_image","arguments":args,"requestState":bad,"inputResponses":{"confirm":{"action":"accept","content":{"confirm":True}}},"_meta":meta},note='tampered state negative probe',timeout=60)
        evidence['tampered_state']={"response_excerpt":redact_text(json.dumps(rt))[:1000],"is_error":bool(rt.get('error') or rt.get('result',{}).get('isError'))}
        r2=st.send('tools/call',{"name":"artcraft_generate_image","arguments":args,"requestState":state,"inputResponses":{"confirm":{"action":"accept","content":{"confirm":True}}},"_meta":meta},note='forced MRTR accept round 2',timeout=90)
        res2=r2.get('result',{}); taskid=res2.get('taskId')
        evidence['round2_accept']={"resultType":res2.get('resultType'),"taskId":taskid,"status":res2.get('status'),"raw_excerpt":redact_text(json.dumps(res2))[:1200]}
        polls=[]; final={}; start=time.time()
        while taskid and time.time()-start<480:
            tg=st.send('tasks/get',{"taskId":taskid,"_meta":meta},note='poll forced-MRTR task',timeout=60)
            final=tg.get('result',{}); status=final.get('status'); polls.append({'at':utcnow(),'status':status})
            if status in ('completed','failed','cancelled'): break
            time.sleep(8)
        evidence['tasks']={"polls":polls,"final_status":final.get('status'),"final_excerpt":redact_text(json.dumps(final))[:2000]}
        txt=json.dumps(final)
        mm=re.search(r'\b(m_[A-Za-z0-9]{6,})\b',txt); media=mm.group(1) if mm else None
        evidence['media_token']=media
        if media:
            dl=st.send('tools/call',{"name":"artcraft_download_media_file","arguments":{"media_token":media,"download_dir":str(vdir),"filename":"11_mrtr_forced_flux_protocol_image.jpg"},"_meta":meta},note='download retained forced-MRTR image',timeout=180)
            fp=vdir/'11_mrtr_forced_flux_protocol_image.jpg'
            evidence['artifact']={"media_token":media,"filename":fp.name,"downloaded":fp.exists() and fp.stat().st_size>0,"size_bytes":fp.stat().st_size if fp.exists() else 0,"sha256":sha256_file(fp) if fp.exists() else None,"download_excerpt":redact_text(json.dumps(dl.get('result',{})))[:1000]}
    finally:
        st.close(); evidence['finished_at']=utcnow(); out.write_text(json.dumps(evidence,indent=2,ensure_ascii=False),encoding='utf-8')
    print('forced MRTR evidence:',out)

if __name__=='__main__': main()
