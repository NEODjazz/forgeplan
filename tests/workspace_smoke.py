"""Persistent, scoped workspace acceptance tests; credentials are test fixtures."""
from pathlib import Path
import atexit, http.client, json, os, subprocess, tempfile, time, uuid
root=Path(__file__).resolve().parents[1]
exe=root/'target/release'/('forgeplan.exe' if os.name=='nt' else 'forgeplan')
checks=0
with tempfile.TemporaryDirectory() as tmp:
 db=str(Path(tmp)/'workspace.sqlite');port=8791
 identities={c*32:{'subject':name,'organization':'alpha','role':role} for c,name,role in [('a','admin','admin'),('e','worker','editor'),('m','manager','manager'),('v','viewer','viewer')]}
 env={**os.environ,'FORGEPLAN_IDENTITIES':json.dumps(identities)}
 def start():
  process=subprocess.Popen([exe,'workspace',str(port),db],env=env,stdout=subprocess.DEVNULL);atexit.register(process.terminate)
  for _ in range(150):
   try:
    request('GET','/api/capabilities',token='a');return process
   except ConnectionRefusedError: time.sleep(.02)
  raise RuntimeError('Workspace failed to start')
 def request(method,path,data=None,token=None,project='demo',org='alpha',key=None):
  headers={'Origin':f'http://127.0.0.1:{port}','Content-Type':'application/json','X-Organization':org,'X-Project-ID':project,'X-Command-ID':key or str(uuid.uuid4())}
  if token:headers['Authorization']='Bearer '+token*32
  c=http.client.HTTPConnection('127.0.0.1',port,timeout=15);body=json.dumps(data) if isinstance(data,(dict,list)) else data;c.request(method,path,body=body.encode() if isinstance(body,str) else body,headers=headers);r=c.getresponse();status=r.status;raw=r.read().decode();c.close()
  try:value=json.loads(raw)
  except json.JSONDecodeError:value=raw
  return status,value
 def check(condition):
  global checks
  assert condition
  checks+=1
 def state():
  code,s=request('GET','/api/state',token='a');assert code==200;return s
 def cmd(commands,role='e',base=None,key=None):
  return request('POST','/api/commands',{'base_revision':state()['revision'] if base is None else base,'commands':commands},token=role,key=key)
 server=start()
 check(request('GET','/api/state')[0]==401)
 check(request('GET','/api/projects',token='a',org='beta')[0]==403)
 check(request('POST','/api/projects',{'id':'demo','title':'Alpha demo'},token='a')[0]==200)
 check(request('GET','/api/state',token='v')[0]==200)
 check(request('POST','/api/commands',{'base_revision':0,'commands':[]},token='v')[0]==403)
 check(request('POST','/api/projects',{'id':'denied'},token='e')[0]==403)
 check(request('POST','/api/publish',{'base_revision':0},token='e')[0]==403)
 key=str(uuid.uuid4());payload=[{'type':'rate','data':{'id':1,'cents_per_hour':6000}}]
 code,first=cmd(payload,base=0,key=key);check(code==200)
 check(cmd(payload,base=0,key=key)==(200,first))
 check(cmd([{'type':'rate','data':{'id':1,'cents_per_hour':1}}],base=0,key=key)[0]==409)
 check(cmd(payload,base=0)[0]==409)
 code,s=cmd([{'type':'baseline','data':{'name':'Approved'}},{'type':'status_date','data':'2026-10-27T09:00'}]);check(code==200)
 check(s['schedule']['management']['evm']['bac_cents']>0)
 check(request('POST','/api/publish',{'base_revision':s['revision']},token='m')[0]==200)
 check(request('GET','/api/published',token='v')[1]['revision']==s['revision'])
 check(request('GET','/api/audit',token='v')[1]['events'][0]['action']=='publish')
 code,entry=request('POST','/api/timesheets',{'task':1,'resource':1,'date':'2026-10-26','minutes':60},token='e');check(code==200)
 check(request('POST','/api/timesheets/approve',{'id':entry['id']},token='e')[0]==403)
 check(request('POST','/api/timesheets/approve',{'id':entry['id']},token='m')[0]==200)
 check(request('POST','/api/timesheets/approve',{'id':entry['id']},token='m')[0]==409)
 check(state()['schedule']['management']['settings']['actuals']['1']['work_basis']==600000)
 check(request('POST','/api/timesheets',{'task':1,'resource':99,'date':'2026-10-26','minutes':60},token='e')[0]==422)
 check(request('GET','/api/timesheets',token='v')[1]['timesheets'][0]['status']=='approved')
 revision=state()['revision'];server.terminate();server.wait(timeout=5);server=start();check(state()['revision']==revision)
 check(request('GET','/api/published',token='v')[1]['revision']<state()['revision'])
 check(request('POST','/api/projects',{'id':'second'},token='a')[0]==200)
 check(len(request('GET','/api/portfolio',token='v')[1]['projects'])==2)
 check(request('GET','/api/state',token='v',project='second')[1]['revision']==0)
 code,xml=request('GET','/api/export.xml',token='v');check(code==200 and bool(xml['warnings']))
 check(request('POST','/api/import.xml',xml['xml'],token='e')[0]==200)
 code,csv=request('GET','/api/export.csv',token='v');check(code==200 and 'duration_minutes' in csv)
 check(request('POST','/api/import.csv',csv,token='e')[0]==200)
 check(request('POST','/api/import.xml','<!DOCTYPE Project><Project/>',token='e')[0]==422)
 code,preview=request('POST','/api/leveling/preview',{'within_float':False},token='e');check(code==200)
 if preview['preview']['unresolved']==0:
  code,leveled=request('POST','/api/leveling/apply',{'base_revision':preview['base_revision'],'within_float':False},token='e');assert code==200,(code,leveled);check(code==200)
  check(all(r['overloaded_minutes']==0 for r in leveled['schedule']['resource_report']['resources']))
  check(request('POST','/api/leveling/apply',{'base_revision':preview['base_revision'],'within_float':False},token='e')[0]==409)
 # Offline pull/push keeps credentials outside documents and rejects a stale base.
 out=Path(tmp)/'offline.fplan';sync_env={**os.environ,'FORGEPLAN_TOKEN':'e'*32}
 subprocess.run([exe,'pull',f'http://127.0.0.1:{port}','alpha','demo',out],env=sync_env,check=True)
 offline=json.loads(out.read_text(encoding='utf-8'));offline['project']['title']='Offline edit';out.write_text(json.dumps(offline),encoding='utf-8')
 subprocess.run([exe,'push',out],env=sync_env,check=True);check(state()['schedule']['title']=='Offline edit')
 old_revision=state()['revision'];cmd([{'type':'fixed_cost','data':{'id':1,'cents':10000}}]);offline['project']['title']='Stale';out.write_text(json.dumps(offline),encoding='utf-8')
 failed=subprocess.run([exe,'push',out],env=sync_env,capture_output=True);check(failed.returncode==1 and b'Revision conflict' in failed.stderr)
 backup=Path(tmp)/'backup.sqlite';subprocess.run([exe,'backup',db,backup],check=True);check(backup.stat().st_size>0)
 import sqlite3
 with sqlite3.connect(backup) as restored:
  check(restored.execute('PRAGMA integrity_check').fetchone()[0]=='ok')
  check(restored.execute("SELECT revision FROM projects WHERE org='alpha' AND id='demo'").fetchone()[0]==state()['revision'])
 check(request('GET','/api/state',token='a',org='not-alpha')[0]==403)
 check(request('POST','/api/commands','{broken',token='e')[0]==422)
 server.terminate();server.wait(timeout=5)
print(f'Workspace acceptance: {checks} checks passed')
