"""Native desktop storage/lifecycle and archive acceptance. No real browser opened."""
from pathlib import Path
import hashlib
import http.client
import importlib.util
import json
import os
import re
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile

root = Path(__file__).resolve().parents[1]
exe = root / 'target/release' / ('forgeplan.exe' if os.name == 'nt' else 'forgeplan')
checks = 0

def check(value):
    global checks
    assert value
    checks += 1

def run(args, **kw):
    return subprocess.run([str(exe), *args], capture_output=True, text=True, encoding="utf-8", timeout=15, **kw)

def fetch(port, method='GET', body=None):
    c = http.client.HTTPConnection('127.0.0.1', port, timeout=5)
    c.request(method, '/api/commands' if body else '/api/state', body=body, headers={'Origin':f'http://127.0.0.1:{port}', 'X-Command-ID':'platform-edit', 'Content-Type':'text/plain'})
    response = c.getresponse()
    assert response.status == 200, response.read()
    value = json.loads(response.read()); c.close(); return value

def start(binary, args, cwd, env, directory):
    log = directory / ('server-' + str(time.time_ns()) + '.txt')
    output = log.open('wb')
    process = subprocess.Popen([str(binary), *args], cwd=cwd, env=env, stdout=output, stderr=output)
    for _ in range(200):
        text = log.read_text(encoding='utf-8', errors='replace')
        match = re.search(r'Open http://127.0.0.1:(\d+)', text)
        if match:
            return process, output, int(match[1])
        if process.poll() is not None:
            output.close(); raise AssertionError(text)
        time.sleep(.025)
    process.terminate(); process.wait(); output.close(); raise AssertionError('Startup timeout')

def stop(process, output):
    process.terminate(); process.wait(timeout=10); output.close()

with tempfile.TemporaryDirectory(prefix='forgeplan-platform-') as tmp:
    directory = Path(tmp)
    data = directory / 'данные проекта with spaces'
    env = {k:v for k,v in os.environ.items() if not k.startswith('FORGEPLAN_')}
    env['FORGEPLAN_DATA_DIR'] = str(data)
    paths = run(['paths'], env=env)
    check(paths.returncode == 0)
    metadata = json.loads(paths.stdout)
    check(Path(metadata['data_dir']) == data and not data.exists())
    check(run(['--version'], env=env).returncode == 0)
    check(run(['paths'], env={**env, 'FORGEPLAN_DATA_DIR':'relative'}).returncode != 0)
    check(run(['desktop','invalid','--no-browser'], env=env).returncode != 0)
    check(run(['desktop','--unknown'], env=env).returncode != 0)
    a = directory / 'cwd a'; b = directory / 'cwd b'; a.mkdir(); b.mkdir()
    process, output, port = start(exe, ['desktop','0','--no-browser'], a, env, directory)
    try:
        state = fetch(port)
        changed = fetch(port, 'POST', f"revision\t{state['revision']}\nupdate\t1\t1440\tPortable task\n")
        check(changed['revision'] > state['revision'])
        check(data.joinpath('forgeplan.sqlite').is_file())
        if os.name != 'nt': check(data.stat().st_mode & 0o077 == 0)
        # Port collision fails and never opens a browser on another service.
        busy = run(['desktop',str(port),str(directory/'collision.sqlite'),'--no-browser'],env=env)
        check(busy.returncode != 0)
    finally: stop(process, output)
    process, output, port = start(exe, ['desktop','0','--no-browser'], b, env, directory)
    try:
        restored = fetch(port)
        check(restored['document'] == changed['document'])
        check(restored['revision'] == changed['revision'])
        check(not a.joinpath('forgeplan.sqlite').exists() and not b.joinpath('forgeplan.sqlite').exists())
    finally: stop(process, output)
    rejected = run(['desktop','0','--no-browser'], env={**env,'FORGEPLAN_BIND':'0.0.0.0'})
    check(rejected.returncode != 0 and 'loopback' in rejected.stderr)
    for url in ['http://projects.example.org','https://user:secret@projects.example.org','file:///tmp/a','https://projects.example.org/?token=x']:
        check(run(['connect',url],env=env).returncode != 0)
    if sys.platform.startswith('linux'):
        handlers = directory / 'handlers'; handlers.mkdir()
        capture = directory / 'browser-url.txt'
        handler = handlers / 'xdg-open'
        handler.write_text('#!/bin/sh\nprintf "%s" "$1" > "$FORGEPLAN_BROWSER_CAPTURE"\n')
        handler.chmod(0o755)
        browser_env = {**env,'PATH':str(handlers)+os.pathsep+env['PATH'],'FORGEPLAN_BROWSER_CAPTURE':str(capture)}
        check(run(['connect','https://projects.example.org'],env=browser_env).returncode == 0)
        check(capture.read_text() == 'https://projects.example.org/')
        capture.unlink()
        process, output, port = start(exe, ['desktop','0'], b, browser_env, directory)
        try:
            for _ in range(200):
                if capture.exists(): break
                time.sleep(.01)
            check(capture.read_text() == f'http://127.0.0.1:{port}/')
            check(fetch(port)['revision'] == changed['revision'])
        finally: stop(process, output)
    # Package and run the extracted binary; path contains spaces and Unicode.
    spec = importlib.util.spec_from_file_location('forgeplan_package',root/'scripts/package-release.py')
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    platform = next(row for row in json.loads((root/'packaging/platforms.json').read_text())['platforms'] if row['os']==metadata['os'] and row['arch']==metadata['arch'])
    archive = module.package(exe, directory/'packages', platform['id'])
    sha = archive.with_name(archive.name+'.sha256').read_text().split()[0]
    check(sha == hashlib.sha256(archive.read_bytes()).hexdigest())
    extracted = directory/'extracted with spaces'; extracted.mkdir()
    if archive.suffix == '.zip':
        with zipfile.ZipFile(archive) as src: src.extractall(extracted)
    else:
        with tarfile.open(archive) as src: src.extractall(extracted, filter='data')
    package = next(extracted.iterdir()); manifest = json.loads((package/'manifest.json').read_text())
    check(all(hashlib.sha256((package/file).read_bytes()).hexdigest()==expected for file,expected in manifest['files_sha256'].items()))
    check(manifest['signed'] is False and manifest['version']==metadata['version'])
    native = package / exe.name
    check(subprocess.run([str(native),'--version'],capture_output=True,timeout=10).returncode == 0)
    launcher = package / ('Start-ForgePlan.command' if metadata['os'] == 'macos' else 'start-forgeplan.sh')
    program, arguments = (native, ['desktop','0','--no-browser']) if os.name == 'nt' or manifest['build'].get('native_client') else (launcher, ['0','--no-browser'])
    process, output, port = start(program, arguments, package, env, directory)
    try: check(fetch(port)['revision'] == changed['revision'])
    finally: stop(process, output)
    try:
        module.package(exe, directory/'packages', platform['id']); raise AssertionError('Overwrite allowed')
    except FileExistsError: check(True)
    wrong = next(row['id'] for row in json.loads((root/'packaging/platforms.json').read_text())['platforms'] if row['os']!=metadata['os'])
    try:
        module.package(exe, directory/'other', wrong); raise AssertionError('Mislabeled candidate allowed')
    except ValueError: check(True)
print(f'Native platform acceptance: {checks} checks passed ({metadata["os"]} {metadata["arch"]})')
