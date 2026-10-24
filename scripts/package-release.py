#!/usr/bin/env python3
"""Package a native, runnable ForgePlan binary; no cross-platform relabeling.

Checksums detect corruption; they are not a release signature.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def package(binary, output, platform_id):
    matrix = json.loads((ROOT / 'packaging/platforms.json').read_text(encoding='utf-8'))['platforms']
    spec = next((row for row in matrix if row['id'] == platform_id), None)
    if spec is None:
        raise ValueError('Unknown platform; see packaging/platforms.json')
    # Run the binary, not the packaging host's Python architecture check. A host
    # running an emulated candidate still reports the actual candidate target.
    result = subprocess.run([str(binary.resolve()), 'build-info'], check=True, capture_output=True, text=True, encoding='utf-8', timeout=15)
    build = json.loads(result.stdout)
    if any(build.get(key) != spec[key] for key in ['os', 'arch', 'target']):
        raise ValueError('Binary target/OS/architecture does not match the selected platform')
    version = build['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:[-+][\w.-]+)?', version):
        raise ValueError('Invalid candidate version')
    cargo_version = re.search(r'^version = "([^"]+)"$', (ROOT / 'Cargo.toml').read_text(), re.M)[1]
    if version != cargo_version:
        raise ValueError('Binary version does not match the source; rebuild first')
    output.mkdir(parents=True, exist_ok=True)
    name = f'ForgePlan-{version}-{platform_id}'
    archive = output / (name + '.' + spec['archive'])
    checksum = output / (archive.name + '.sha256')
    if archive.exists() or checksum.exists():
        raise FileExistsError('Release output exists; use a fresh output directory')
    with tempfile.TemporaryDirectory(prefix='forgeplan-package-') as temporary:
        stage = Path(temporary) / name
        stage.mkdir()
        shutil.copy2(binary, stage / ('forgeplan.exe' if spec['os'] == 'windows' else 'forgeplan'))
        for file in ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']:
            source = ROOT / file
            if source.exists(): shutil.copy2(source, stage / file)
        for directory in ['docs', 'third_party_licenses', 'examples', 'schemas']:
            shutil.copytree(ROOT / directory, stage / directory)
        shutil.copy2(ROOT / 'packaging/platforms.json', stage / 'platforms.json')
        native = build.get('native_client', False)
        mode = 'app' if native else 'desktop'
        if spec['os'] == 'windows':
            script = '@echo off\r\nstart "" "%~dp0forgeplan.exe" app\r\n' if native else '@echo off\r\n"%~dp0forgeplan.exe" desktop %*\r\nif errorlevel 1 pause\r\n'
            (stage / 'Start-ForgePlan.cmd').write_bytes(script.encode('ascii'))
        else:
            launcher = stage / ('Start-ForgePlan.command' if spec['os'] == 'macos' else 'start-forgeplan.sh')
            launcher.write_text('#!/bin/sh\nset -eu\napp_dir=$(CDPATH= cd -P "$(dirname "$0")" && pwd)\nexec "$app_dir/forgeplan" '+mode+' "$@"\n', encoding='utf-8')
            launcher.chmod(0o755)
            (stage / 'forgeplan').chmod(0o755)
        if native and spec['os'] == 'macos':
            bundle = stage / 'ForgePlan.app' / 'Contents'
            (bundle / 'MacOS').mkdir(parents=True)
            shutil.move(stage / 'forgeplan', bundle / 'MacOS' / 'forgeplan')
            info = {'CFBundleName':'ForgePlan', 'CFBundleDisplayName':'ForgePlan', 'CFBundleIdentifier':'org.forgeplan.desktop', 'CFBundlePackageType':'APPL', 'CFBundleExecutable':'forgeplan', 'CFBundleShortVersionString':version, 'CFBundleVersion':version, 'LSMinimumSystemVersion':'13.0', 'NSHighResolutionCapable':True, 'NSAppTransportSecurity':{'NSAllowsLocalNetworking':True,'NSExceptionDomains':{'127.0.0.1':{'NSExceptionAllowsInsecureHTTPLoads':True}}}}
            (bundle / 'Info.plist').write_bytes(plistlib.dumps(info))
            wrapper = stage / 'forgeplan'
            wrapper.write_text('#!/bin/sh\nset -eu\napp_dir=$(CDPATH= cd -P "$(dirname "$0")" && pwd)\nexec "$app_dir/ForgePlan.app/Contents/MacOS/forgeplan" "$@"\n')
            wrapper.chmod(0o755)
        usage = ('Native client: run forgeplan app (or double-click the executable on Windows).\nOn macOS open ForgePlan.app. Choose Local projects or Corporate server in the window.\nLocal mode starts an owned, token-protected loopback process; no external server or internet is required.\nClosing the window normally stops the owned process. F6 returns home; Ctrl/Cmd+S applies changes; Ctrl/Cmd+Q exits.\nRemote mode requires server connectivity; local and remote projects are separate.\nAutomatic offline cache/outbox/merge is not implemented.\nLinux requires GTK3 and WebKitGTK 4.1 runtime; Windows requires WebView2 Runtime.\n') if native else ('Browser launcher: run forgeplan desktop. Keep the console open; Ctrl+C stops it.\nDefault port 8787; choose a different port if it is busy.\n')
        (stage / 'README.txt').write_text(f'''ForgePlan {version} Alpha — {platform_id}

Extract the entire package to a directory writable by you.
{usage}
Start-ForgePlan launchers are included. This candidate is unsigned.

Data lives in the OS user data directory, not beside the executable.
Run forgeplan paths to locate it; FORGEPLAN_DATA_DIR overrides it (absolute path).
Existing data from 0.9: pass the old database path explicitly; no auto-import.
Do not move SQLite/WAL files while the server is running. Use the backup command.
Upgrade: stop the server, back up data, extract a new package into a new directory.
Remove: delete the extracted package. User data is retained separately.

Corporate server: forgeplan connect https://projects.example.org
This opens the web client. Sign in through the browser; no local DB is created.
No automatic updater, MSI/DMG installer, signed/notarized app or system background service.
This candidate is unsigned; OS security checks still apply.
Read docs/NATIVE_CLIENT.md, docs/PLATFORMS.md, docs/SECURITY.md, docs/AUTH.md before team deployment.
Platform baseline: {spec['baseline']}
''', encoding='utf-8')
        hashes = {str(file.relative_to(stage)).replace('\\', '/'): digest(file) for file in sorted(stage.rglob('*')) if file.is_file()}
        manifest = {'format':'forgeplan-release','schema_version':1,'version':version,'platform':spec,'signed':False,'build':build,'files_sha256':hashes}
        (stage / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
        # Write into a temporary file then rename only on success.
        tmp_archive = Path(temporary) / archive.name
        if spec['archive'] == 'zip':
            with zipfile.ZipFile(tmp_archive, 'w', zipfile.ZIP_DEFLATED, strict_timestamps=False) as dest:
                for file in sorted(stage.rglob('*')):
                    if file.is_file(): dest.write(file, file.relative_to(stage.parent))
        else:
            with tarfile.open(tmp_archive, 'w:gz') as dest:
                dest.add(stage, arcname=name)
        shutil.copyfile(tmp_archive, archive)
    checksum.write_text(f'{digest(archive)}  {archive.name}\n', encoding='ascii')
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform', required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT / 'dist')
    args = parser.parse_args()
    print(package(args.binary, args.output.resolve(), args.platform))


if __name__ == '__main__':
    main()
