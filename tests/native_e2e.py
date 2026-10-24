"""Linux native-window acceptance: xdotool + Xvfb, no browser test bridge.

Run after cargo build --release --features native-client; use xvfb-run -a.
FORGEPLAN_TEST_TCP_DISPLAY enables a TCP Xvfb for restricted test environments.
"""
from pathlib import Path
import http.client
import json
import os
import re
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time

if not sys.platform.startswith('linux'):
    raise SystemExit('This GUI driver requires Linux/X11; other platforms need native GUI acceptance.')
root=Path(__file__).resolve().parents[1]
exe=root/'target/release/forgeplan'
checks=0

def check(condition):
    global checks
    assert condition
    checks+=1

def eventually(test, label, timeout=12):
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        try:
            value=test()
            if value:return value
        except (OSError, sqlite3.Error, json.JSONDecodeError):pass
        time.sleep(.05)
    raise AssertionError('Timeout: '+label)

def request(url):
    from urllib.parse import urlsplit
    parts=urlsplit(url);c=http.client.HTTPConnection(parts.hostname,parts.port,timeout=2)
    c.request('GET','/api/state');r=c.getresponse();status=r.status;r.read();c.close();return status

def closed(url):
    try:request(url);return False
    except OSError:return True

def snapshot(path):
    with sqlite3.connect(path) as db:
        return json.loads(db.execute("SELECT snapshot FROM projects WHERE org='local' AND id='demo'").fetchone()[0])

with tempfile.TemporaryDirectory(prefix='forgeplan-native-ui-') as temporary:
    directory=Path(temporary);data=directory/'данные with spaces';db=data/'forgeplan.sqlite'
    env={k:v for k,v in os.environ.items() if not k.startswith('FORGEPLAN_')}
    env['FORGEPLAN_DATA_DIR']=str(data)
    xvfb=None;app=None;remote=None;output=None
    if os.environ.get('FORGEPLAN_TEST_TCP_DISPLAY'):
        display=os.environ['FORGEPLAN_TEST_TCP_DISPLAY'];int(display)
        xvfb=subprocess.Popen(['Xvfb',':'+display,'-screen','0','1440x1000x24','-ac','-nolisten','unix','-nolisten','local','-listen','tcp'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        env['DISPLAY']='127.0.0.1:'+display
    if not env.get('DISPLAY'):raise RuntimeError('Use xvfb-run -a or set DISPLAY')
    def xdo(*args):
        result=subprocess.run(['xdotool',*map(str,args)],env=env,capture_output=True,text=True,encoding='utf-8',timeout=5)
        return result.stdout.strip()
    def window():
        ids=xdo('search','--onlyvisible','--name','^ForgePlan —').splitlines()
        return ids[-1] if ids else None
    def title_contains(text):
        wid=window()
        return wid if wid and text in xdo('getwindowname',wid) else None
    def focus():
        wid=eventually(window,'native window');xdo('windowfocus','--sync',wid);return wid
    def key(value):focus();xdo('key','--clearmodifiers',value)
    def click(x,y):focus();xdo('mousemove',x,y);xdo('click',1)
    def screenshot(name):
        location=os.environ.get('FORGEPLAN_QA_SCREENSHOTS')
        if location:
            from PIL import ImageGrab
            dest=Path(location);dest.mkdir(parents=True,exist_ok=True)
            ImageGrab.grab(xdisplay=env['DISPLAY']).save(dest/(name+'.png'))
    def launch():
        log=directory/('client-'+str(time.time_ns())+'.log');handle=log.open('wb')
        process=subprocess.Popen([exe,'app'],env=env,stdout=handle,stderr=handle,start_new_session=True)
        eventually(lambda:title_contains('выбор режима'),'home screen');time.sleep(.5)
        return process,handle,log
    def choose_local(log):
        click(480,650);eventually(lambda:title_contains('локальные проекты'),'local editor')
        text=eventually(lambda:log.read_text(encoding='utf-8',errors='replace') if 'Native local session ready:' in log.read_text(encoding='utf-8',errors='replace') else None,'local readiness')
        url=re.findall(r'Native local session ready: (http://127.0.0.1:\d+/)',text)[-1]
        eventually(lambda:db.exists(),'local database');time.sleep(1);return url
    def go_home():key('F6');eventually(lambda:title_contains('выбор режима'),'return home');time.sleep(.4)
    def quit_client():
        key('ctrl+q');eventually(lambda:app.poll() is not None,'normal client quit');check(app.returncode==0)
    try:
        if xvfb:time.sleep(.6)
        build=json.loads(subprocess.check_output([exe,'build-info'],env=env));check(build['native_client'] is True)
        app,output,log=launch();check(not db.exists());screenshot('native-home')
        local=choose_local(log);check(request(local)==401);check(snapshot(db)['revision']==0)
        screenshot('native-local')
        # Fixed 1440x1000 Linux reference viewport. Type into the project title,
        # then invoke the desktop Save shortcut instead of calling the API.
        click(300,570);key('ctrl+a');xdo('type','--clearmodifiers','Native offline plan');key('ctrl+s')
        eventually(lambda:snapshot(db)['project']['title']=='Native offline plan','edit committed from native window')
        check(snapshot(db)['revision']==1);screenshot('native-edited')
        go_home();eventually(lambda:closed(local),'owned process stopped on home');check(True)
        local=choose_local(log);check(snapshot(db)['project']['title']=='Native offline plan');check(request(local)==401)
        quit_client();eventually(lambda:closed(local),'owned process stopped on quit');check(True);output.close()
        app,output,log=launch();local=choose_local(log);check(snapshot(db)['revision']==1);go_home();eventually(lambda:closed(local),'second owned process stopped')
        # A separate workspace represents the corporate server; the client never
        # owns or terminates it and the two databases remain independent.
        remote=subprocess.Popen([exe,'workspace','0',directory/'corporate.sqlite'],env=env,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
        line=remote.stdout.readline().decode();server=line.strip().removeprefix('Open ');check(request(server)==200)
        click(970,596);key('ctrl+a');xdo('type','--clearmodifiers',server);key('Return')
        eventually(lambda:title_contains('корпоративный сервер'),'corporate editor');time.sleep(1);screenshot('native-remote')
        preferences=json.loads((data/'client-settings.json').read_text(encoding='utf-8'));check(preferences=={'server':server+'/'})
        check(snapshot(db)['project']['title']=='Native offline plan')
        go_home();check(request(server)==200)
        # Reject non-TLS remote URLs without replacing the saved connection.
        click(970,596);key('ctrl+a');xdo('type','--clearmodifiers','http://projects.invalid');key('Return');time.sleep(.7)
        check(bool(title_contains('выбор режима')))
        check(json.loads((data/'client-settings.json').read_text())==preferences)
        # Remote IPC cannot request local files/processes; its only accepted
        # action is Home (also covered by the native unit boundary tests).
        quit_client();check(request(server)==200)
        print(f'Native window E2E: {checks} checks passed (offline edit/restart, private session, home/quit cleanup, remote mode and URL validation).')
    finally:
        if app:
            try:os.killpg(app.pid,signal.SIGTERM)
            except ProcessLookupError:pass
            if app.poll() is None:app.wait(timeout=10)
        if output:output.close()
        if remote:remote.terminate();remote.wait(timeout=10)
        if xvfb:xvfb.terminate();xvfb.wait(timeout=10)
