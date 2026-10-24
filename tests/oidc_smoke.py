"""Local RSA OIDC/JWKS provider. No real credentials or remote IdP are used."""
import base64, hashlib, http.client, json, os, subprocess, tempfile, threading, time, urllib.parse
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
root=Path(__file__).resolve().parents[1];exe=root/'target/release'/('forgeplan.exe' if os.name=='nt' else 'forgeplan');checks=0
b64=lambda data:base64.urlsafe_b64encode(data).rstrip(b'=').decode()
with tempfile.TemporaryDirectory() as tmp:
 keys={}
 for kid in ['one','two']:
  key=Path(tmp)/(kid+'.pem');subprocess.run(['openssl','genpkey','-algorithm','RSA','-pkeyopt','rsa_keygen_bits:2048','-out',key],check=True,capture_output=True)
  modulus=subprocess.check_output(['openssl','rsa','-in',key,'-noout','-modulus']).decode().strip().split('=')[1]
  keys[kid]=(key,{'kty':'RSA','kid':kid,'use':'sig','alg':'RS256','n':b64(bytes.fromhex(modulus)),'e':'AQAB'})
 active=['one'];challenges={};requests={'jwks':0,'pkce':0};issuer='';app_origin='http://127.0.0.1:8793'
 def token(**updates):
  kid=updates.pop('kid',active[0]);claims={'iss':issuer,'aud':'forgeplan-api','sub':'oidc-user','exp':int(time.time())+300,'iat':int(time.time()),'realm_access':{'roles':['forgeplan_admin']}}
  for key,val in updates.items():
   if val is None:claims.pop(key,None)
   else:claims[key]=val
  header=b64(json.dumps({'alg':'RS256','kid':kid}).encode());body=b64(json.dumps(claims).encode());message=(header+'.'+body).encode();signature=subprocess.check_output(['openssl','dgst','-sha256','-sign',keys[kid][0]],input=message);return message.decode()+'.'+b64(signature)
 class IdP(BaseHTTPRequestHandler):
  def log_message(self,*args):pass
  def reply(self,value,status=200):
   body=json.dumps(value).encode();self.send_response(status);self.send_header('Content-Type','application/json');self.send_header('Access-Control-Allow-Origin',app_origin);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
  def do_GET(self):
   path=urllib.parse.urlsplit(self.path)
   if path.path.endswith('/.well-known/openid-configuration'):self.reply({'issuer':issuer,'authorization_endpoint':issuer+'/authorize','token_endpoint':issuer+'/token','jwks_uri':f'http://127.0.0.1:{self.server.server_port}/keys'})
   elif path.path=='/keys':requests['jwks']+=1;self.reply({'keys':[keys[k][1] for k in active]})
   elif path.path.endswith('/authorize'):
    q=urllib.parse.parse_qs(path.query);challenges['fixed-code']=q['code_challenge'][0];target=q['redirect_uri'][0]+'?code=fixed-code&state='+q['state'][0];self.send_response(302);self.send_header('Location',target);self.end_headers()
   else:self.reply({},404)
  def do_POST(self):
   q=urllib.parse.parse_qs(self.rfile.read(int(self.headers['Content-Length'])).decode());verifier=q.get('code_verifier',[''])[0];challenge=b64(hashlib.sha256(verifier.encode()).digest());valid=q.get('code',[''])[0]=='fixed-code' and challenges.get('fixed-code')==challenge and q.get('client_id',[''])[0]=='forgeplan-browser';requests['pkce']+=int(valid)
   self.reply({'access_token':token(),'token_type':'Bearer','expires_in':300} if valid else {'error':'invalid_grant'},200 if valid else 400)
 idp=ThreadingHTTPServer(('127.0.0.1',0),IdP);issuer=f'http://127.0.0.1:{idp.server_port}/realm';thread=threading.Thread(target=idp.serve_forever,daemon=True);thread.start()
 env={**os.environ,'FORGEPLAN_OIDC_ISSUER':issuer,'FORGEPLAN_OIDC_AUDIENCE':'forgeplan-api','FORGEPLAN_OIDC_CLIENT_ID':'forgeplan-browser','FORGEPLAN_OIDC_ORGANIZATION':'alpha','FORGEPLAN_OIDC_ALLOW_LOOPBACK':'1'};env.pop('FORGEPLAN_IDENTITIES',None)
 server=subprocess.Popen([exe,'workspace','8793',str(Path(tmp)/'workspace.sqlite')],env=env,stdout=subprocess.DEVNULL)
 def call(path='/api/capabilities',jwt=None,org='alpha',method='GET',body=None):
  c=http.client.HTTPConnection('127.0.0.1',8793,timeout=10);headers={'X-Organization':org,'Origin':app_origin,'Content-Type':'application/json'}
  if jwt:headers['Authorization']='Bearer '+jwt
  c.request(method,path,body=json.dumps(body) if body is not None else None,headers=headers);r=c.getresponse();result=r.status;value=json.loads(r.read());c.close();return result,value
 def check(condition):
  global checks
  assert condition
  checks+=1
 try:
  for _ in range(150):
   try:call('/api/auth/config');break
   except ConnectionRefusedError:time.sleep(.02)
  else:raise RuntimeError('OIDC server did not start')
  check(call('/api/auth/config')[0]==200);check(call()[0]==401)
  valid=token();check(call(jwt=valid)[0]==200);check(call(jwt=valid)[1]['role']=='admin');check(call(jwt=valid)[1]['authentication']=='oidc')
  check(call(jwt=token(realm_access=None,roles=['forgeplan_viewer']))[1]['role']=='viewer')
  check(call('/api/projects',token(realm_access=None,roles=['forgeplan_viewer']),method='POST',body={'id':'nope'})[0]==403)
  check(call(jwt=token(aud=['other','forgeplan-api']))[0]==200)
  for jwt in [token(aud='another-api'),token(iss=issuer+'/wrong'),token(exp=int(time.time())-120),token(exp=None),token(iat=int(time.time())+600),token(sub=''),token(kid='one',exp='bad')]:check(call(jwt=jwt)[0]==401)
  forged=valid[:-5]+'AAAAA';check(call(jwt=forged)[0]==401)
  check(call(jwt=token(realm_access={'roles':['admin']}))[0]==403)
  check(call(jwt=token(organization='other'))[0]==403);check(call(jwt=valid,org='other')[0]==403)
  check(call('/api/projects',valid,method='POST',body={'id':'demo','title':'OIDC project'})[0]==200)
  active[:]=['two'];time.sleep(1.1);check(call(jwt=token(kid='two'))[0]==200);check(requests['jwks']>=2)
  before=requests['jwks'];unknown=valid.split('.');unknown[0]=b64(json.dumps({'alg':'RS256','kid':'missing'}).encode());unknown='.'.join(unknown)
  for _ in range(5):check(call(jwt=unknown)[0]==401)
  check(requests['jwks']==before)
  if os.environ.get('FORGEPLAN_RUN_OIDC_BROWSER')=='1':subprocess.run(['node',str(root/'tests/oidc_browser.cjs')],env={**env,'FORGEPLAN_TEST_ORIGIN':app_origin},check=True);check(requests['pkce']==1)
 finally:server.terminate();server.wait(timeout=5);idp.shutdown();idp.server_close()
print(f'OIDC/JWKS acceptance: {checks} checks passed')
