"""Separate downstream-only rename transaction. Never touches browser storage or official A+B."""
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import uuid
from check_upstream_update import atomic_json, file_digest

IDENTIFIER = 'ai.local.mingle.ccbaga.environments'


def verify_app(app):
    info = plistlib.loads((app/'Contents/Info.plist').read_bytes())
    if app.is_symlink() or info.get('CFBundleIdentifier') != IDENTIFIER or info.get('CFBundleExecutable') != 'requirement-french':
        raise RuntimeError('Downstream app identity mismatch')
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True, capture_output=True)


def is_running(app):
    executable = str(app/'Contents/MacOS/requirement-french')
    rows = subprocess.run(['ps', '-axo', 'comm='], check=True, capture_output=True, text=True).stdout.splitlines()
    return executable in [row.strip() for row in rows]


def install(staged, destination, state_dir, acceptance, verifier=verify_app, running=is_running, checkpoint=lambda _: None):
    staged, destination, state_dir = Path(staged), Path(destination), Path(state_dir)
    if destination.name != 'CCBaga 多环境.app' or destination.is_symlink() or not destination.exists():
        raise RuntimeError('Only an existing downstream app may be replaced')
    verifier(staged); verifier(destination)
    if not acceptance.get('native_gui_accepted') or acceptance.get('binary_sha256') != file_digest(staged/'Contents/MacOS/requirement-french'):
        raise RuntimeError('A matching native GUI/lifecycle acceptance receipt is required')
    if running(destination):
        raise RuntimeError('Downstream application is running; retain candidate')
    state_dir.mkdir(parents=True, exist_ok=True)
    journal = state_dir/'install-transaction.json'
    if journal.exists():
        previous = json.loads(journal.read_text())
        if previous.get('phase') not in ('committed', 'rolled_back'):
            raise RuntimeError('Unfinished installation requires recovery first')
        os.replace(journal, state_dir/('install-'+uuid.uuid4().hex+'.json'))
    stage = destination.parent/('.mingle-ccbaga-'+uuid.uuid4().hex)
    stage.mkdir(mode=0o700)
    new, old = stage/'new.app', stage/'old.app'
    shutil.copytree(staged, new, symlinks=True)
    verifier(new)
    if running(destination):
        raise RuntimeError('Application started during preparation')
    data = {'schema': 1, 'destination': str(destination), 'stage': str(stage), 'phase': 'prepared'}
    atomic_json(journal, data)
    try:
        os.replace(destination, old)
        checkpoint('old_saved')
        os.replace(new, destination)
        verifier(destination)
        checkpoint('new_installed')
        data['phase'] = 'committed'
        atomic_json(journal, data)
    except BaseException:
        if old.exists():
            if destination.exists():
                os.replace(destination, stage/'failed.app')
            os.replace(old, destination)
        data['phase'] = 'rolled_back'
        atomic_json(journal, data)
        raise
    # Keep old.app and journal as a full rollback. No profile path is accepted by this function.
    return data


def recover(state_dir, destination, verifier=verify_app, running=is_running):
    """Explicit recovery only; validate stored paths before restoring a saved application."""
    state_dir, destination = Path(state_dir), Path(destination)
    journal = state_dir/'install-transaction.json'
    data = json.loads(journal.read_text())
    stage = Path(data['stage'])
    if (destination.name != 'CCBaga 多环境.app' or data['destination'] != str(destination)
            or stage.parent != destination.parent or not stage.name.startswith('.mingle-ccbaga-')
            or stage.is_symlink() or stage.stat().st_uid != os.getuid()):
        raise RuntimeError('Untrusted recovery journal')
    if running(destination):
        raise RuntimeError('Application is running; recovery deferred')
    if data['phase'] == 'committed':
        verifier(destination)
        return data
    old = stage/'old.app'
    if old.exists():
        verifier(old)
        if destination.exists():
            os.replace(destination, stage/('retained-'+uuid.uuid4().hex+'.app'))
        os.replace(old, destination)
    verifier(destination)
    data['phase']='rolled_back'
    atomic_json(journal,data)
    return data
