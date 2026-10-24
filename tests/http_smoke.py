"""Local adapter integration tests. Build the release binary first."""
import atexit
import http.client
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

root = Path(__file__).resolve().parents[1]
exe = root / 'target/release' / ('forgeplan.exe' if os.name == 'nt' else 'forgeplan')
server = subprocess.Popen([exe, 'serve', '8787'], stdout=subprocess.DEVNULL)
atexit.register(server.terminate)
headers = {'Content-Type': 'text/plain; charset=utf-8', 'Origin': 'http://127.0.0.1:8787'}
checks = 0

def request(method, path, body=None, extra=None):
    conn = http.client.HTTPConnection('127.0.0.1', 8787, timeout=5)
    conn.request(method, path, body=body.encode() if isinstance(body, str) else body, headers=extra or {})
    response = conn.getresponse()
    code, data = response.status, response.read()
    conn.close()
    return code, data

def check(condition):
    global checks
    assert condition
    checks += 1

def state():
    code, data = request('GET', '/api/state')
    assert code == 200
    return json.loads(data)

def command(rows, expected=200, revision=None):
    revision = state()['revision'] if revision is None else revision
    code, data = request('POST', '/api/commands', f'revision\t{revision}\n{rows}\n', headers)
    check(code == expected)
    return json.loads(data)

def task(s, task_id):
    return next(t for t in s['schedule']['tasks'] if t['id'] == task_id)

for _ in range(100):
    try:
        check(request('GET', '/')[0] == 200)
        break
    except ConnectionRefusedError:
        time.sleep(.02)
else:
    raise RuntimeError('Server not ready')

initial = state()
check(initial['revision'] == 0 and initial['next_id'] == 21)
check(task(initial, 10)['duration'] == 3840 and task(initial, 10)['float'] is None)
check(task(initial, 3)['wbs'] == '1.3' and task(initial, 3)['float'] == 960)
check(request('GET', '/app.js')[0] == 200)
changed = command('update\t2\t3840\tDesign')
check(changed['schedule']['working_days'] == 17 and changed['can_undo'])
command('update\t2\t4320\tBad\nmove\t10\t10\t-', expected=422)
check(state()['document'] == changed['document'] and state()['revision'] == changed['revision'])
command('undo', expected=409, revision=0)
check(state()['revision'] == changed['revision'])
undone = command('undo')
check(task(undone, 2)['duration'] == 2400 and undone['can_redo'])
redone = command('redo')
check(task(redone, 2)['duration'] == 3840)
command('predecessors\t1\t6:FS:0', expected=422)
check(state()['document'] == redone['document'])
moved = command('move\t3\t20\t4')
check(task(moved, 3)['parent'] == 20 and task(moved, 3)['wbs'] == '2.1')
command('undo')
deleted = command('delete\t10')
check({t['id'] for t in deleted['schedule']['tasks']} == {20, 4, 5, 6})
restored = command('undo')
check(task(restored, 1)['parent'] == 10 and len(restored['schedule']['tasks']) == 8)
command('predecessors\t4\t10:FS:0', expected=422)
command('add\t21\t-\t-\tsummary\t0\tPhase\nadd\t22\t21\t-\ttask\t2\tChild')
check(task(state(), 22)['wbs'] == '3.1')
command('move\t21\t21\t-', expected=422)
command('undo')
command('add\t21\t-\t-\ttask\t1\tReused', expected=422)
legacy = (root / 'examples/legacy-v1.fplan').read_text(encoding='utf-8')
imported = command('replace\n' + legacy)
check(len(imported['schedule']['tasks']) == 6 and json.loads(imported['document'])['schema_version'] == 1)
command('undo')
# Minute calendars, date constraints and soft deadline through the real API.
reset = command('replace\n' + (root / 'examples/infrastructure.fplan').read_text(encoding='utf-8'))
constrained = command('dates\t3\tSNET\t2026-10-30T10:15\t2026-10-30T11:00')
check(task(constrained, 3)['start'] == '2026-10-30T10:15+03:00')
check(task(constrained, 3)['deadline_missed'])
check(any('SNET' in reason for reason in task(constrained, 3)['reasons']))
command('undo')
command('dates\t2\tMSO\t2026-11-01T10:00\t-', expected=422)
week = ';'.join(['09:00-13:00,14:00-18:00']*5 + ['-','-'])
calendar_changed = command('calendar\t13:15\t+03:00\t' + week + '\t2026-10-31=10:00-14:00')
check(task(calendar_changed, 1)['start'] == '2026-10-26T14:00+03:00')
check(calendar_changed['schedule']['unit'] == 'working_minutes')
command('calendar\t09:00\tEurope/Moscow\t' + week + '\t-', expected=422)
command('undo')
# Resource creation, assignment, availability, exact report, undo and rollback.
resource_state = command('replace\n' + (root / 'examples/infrastructure.fplan').read_text(encoding='utf-8'))
loads = resource_state['schedule']['resource_report']['resources']
check(len(loads) == 3 and loads[0]['overloaded_minutes'] > 0)
check(loads[0]['planned_minutes'] < loads[0]['capacity_minutes'])
schedule_dates = [(t['id'], t['start'], t['finish']) for t in resource_state['schedule']['tasks']]
updated_resource = command('resource-update\t1\tperson\t10000\tEngineer\t2026-10-30..2026-10-30:0')
check([(t['id'], t['start'], t['finish']) for t in updated_resource['schedule']['tasks']] == schedule_dates)
check(updated_resource['schedule']['resource_report']['resources'][0]['capacity_minutes'] < loads[0]['capacity_minutes'])
command('undo')
assigned = command('assignments\t3\t1:5000,2:10000')
check(task(assigned, 3)['work_minutes'] == 2160)
command('undo')
previous = state()
command('assignments\t3\t1:10000,1:5000', expected=422)
check(state()['document'] == previous['document'] and state()['revision'] == previous['revision'])
deleted_resource = command('resource-delete\t1')
check(all(a['resource'] != 1 for a in deleted_resource['schedule']['resource_report']['assignments']))
command('undo')
check(state()['document'] == previous['document'])
new_id = state()['next_resource_id']
command(f'resource-add\t{new_id}\tgeneric\t20000\tNew team\t-')
command(f'assignments\t3\t{new_id}:3333')
check(task(state(), 3)['work_minutes'] == 479.952)
command('undo')
command('undo')
command(f'resource-add\t{new_id}\tperson\t10000\tReused\t-', expected=422)
command('resource-update\t1\tperson\t10000\tBad\t2026-10-26..2026-10-28:0;2026-10-28..2026-10-30:5000', expected=422)
for body, extra in [('revision\t0\nundo', {}), ('revision\t0\nundo', {**headers, 'Origin': 'https://evil.example'})]:
    check(request('POST', '/api/commands', body, extra)[0] == 403)
check(request('GET', '/', extra={'Host': 'evil.example'})[0] == 403)
check(request('GET', '/missing')[0] == 404)
check(request('POST', '/api/commands', extra={**headers, 'Content-Length': '8000001'})[0] == 400)
check(request('GET', '/api/state')[0] == 200)
code, data = request('POST', '/api/schedule', legacy, headers)
check(code == 200 and json.loads(data)['working_days'] == 14)
check(request('GET', '/api/project')[1].decode() == state()['document'])
with tempfile.TemporaryDirectory() as directory:
    file = Path(directory) / 'demo.fplan'
    subprocess.run([exe, 'demo', str(file)], check=True)
    check(json.loads(file.read_text(encoding='utf-8'))['format'] == 'forgeplan')
    check(subprocess.run([exe, 'demo', str(file)], capture_output=True).returncode == 1)
    out = subprocess.check_output([exe, 'check', str(file)])
    check(json.loads(out)['working_days'] == 14)
print(f'HTTP/CLI integration: {checks} checks passed')
