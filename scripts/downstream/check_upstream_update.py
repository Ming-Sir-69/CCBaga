#!/usr/bin/env python3
"""Fetch and preflight an explicitly requested upstream update; no daemon, scheduler or installation."""
import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import plistlib


def run_git(repo, *args, timeout=45):
    env = dict(os.environ, GIT_TERMINAL_PROMPT='0', GIT_OPTIONAL_LOCKS='0')
    return subprocess.run(['git', '-C', str(repo), *args], env=env, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)


def require(result):
    if result.returncode:
        raise RuntimeError((result.stderr or result.stdout or 'Git command failed').strip()[:800])
    return result.stdout.strip()


def file_digest(path):
    if path.is_symlink():
        return 'symlink:' + os.readlink(path)
    if not path.is_file():
        return None
    return hashlib.sha256(path.read_bytes()).hexdigest()


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(path.name + '.pending')
    temp.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    os.replace(temp, path)


def inspect(repo, downstream, upstream, manifest=None):
    """Pure Git-object merge preflight; merge-tree writes objects but never index/worktree files."""
    down = require(run_git(repo, 'rev-parse', '--verify', downstream + '^{commit}'))
    up = require(run_git(repo, 'rev-parse', '--verify', upstream + '^{commit}'))
    result = {'downstream': down, 'upstream': up, 'conflicts': [], 'working_tree_changed': []}
    if manifest is not None:
        changed = [name for name, digest in manifest.items() if file_digest(repo / name) != digest]
        extra = require(run_git(repo, 'ls-files', '--others', '--exclude-standard')).splitlines()
        changed += [name for name in extra if name not in manifest]
        result['working_tree_changed'] = sorted(set(changed))
    if run_git(repo, 'merge-base', '--is-ancestor', up, down).returncode == 0:
        result['state'] = 'up_to_date'
    else:
        merged = run_git(repo, 'merge-tree', '--write-tree', '--name-only', down, up)
        lines = merged.stdout.splitlines()
        if merged.returncode not in (0, 1) or not lines:
            raise RuntimeError((merged.stderr or merged.stdout or 'Merge preflight failed')[:800])
        result['candidate_tree'] = lines[0]
        if merged.returncode == 0:
            result['state'] = 'merge_clean_pending_tests'
        else:
            result['state'] = 'conflict'
            for line in lines[1:]:
                if not line:
                    break
                result['conflicts'].append(line)
            result['merge_details'] = merged.stdout[:16000]
    if result['working_tree_changed'] and result['state'] != 'conflict':
        result['saved_ref_state'] = result['state']
        result['state'] = 'local_changes'
    return result


def fingerprint(report):
    # Timestamps do not create repeated popups for the same issue.
    fields = {k: report.get(k) for k in ['state', 'upstream', 'downstream', 'conflicts', 'working_tree_changed', 'detection_points', 'error']}
    return hashlib.sha256(json.dumps(fields, sort_keys=True).encode()).hexdigest()


def notification(report):
    state = report['state']
    titles = {'conflict': 'CCBaga：上游更新存在合并冲突', 'error': 'CCBaga：上游检查失败',
              'validation_failed': 'CCBaga：兼容验证失败', 'local_changes': 'CCBaga：本地修改尚未登记', 'merge_clean_pending_tests': 'CCBaga：上游有更新，等待验证'}
    summary = {
        'validation_failed': '验证未通过，请检查 detection_points 与日志。当前应用和资料未更改。',
        'conflict': '已保留当前可用版本。请让 Agent 查看报告并处理冲突。',
        'error': '检查未完成，不能据此判断兼容。当前应用和资料未更改。',
        'local_changes': '检查使用的是已保存的下游版本。请让 Agent 验证并登记新修改。',
        'merge_clean_pending_tests': '文本合并无冲突，但尚未运行构建和功能验收。当前应用未更新。',
    }.get(state, '')
    return titles.get(state, 'CCBaga 更新检查'), summary


def check(config_path, fetch=True, notify=True):
    config_path = config_path.resolve()
    config = json.loads(config_path.read_text())
    repo = (config_path.parent / config['repo']).resolve()
    state_dir = (config_path.parent / config['state_dir']).resolve()
    state_dir.mkdir(parents=True, exist_ok=True)
    lock = (state_dir / 'update.lock').open('a')
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        return {'state': 'already_running'}
    state_file = state_dir / 'status.json'
    try:
        old = json.loads(state_file.read_text())
    except (OSError, ValueError):
        old = {}
    try:
        remote = config['remote']
        actual_url = require(run_git(repo, 'remote', 'get-url', remote))
        if actual_url != config['expected_remote_url']:
            raise RuntimeError('Upstream remote URL differs from the configured repository; fetch was refused.')
        if fetch:
            require(run_git(repo, 'fetch', '--no-tags', remote, config['upstream_branch'], timeout=60))
        manifest = json.loads((config_path.parent / config['manifest']).read_text())
        report = inspect(repo, config['downstream_ref'], remote + '/' + config['upstream_branch'], manifest)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired) as error:
        report = {'state': 'error', 'error': str(error)[:1000], 'conflicts': []}
    report['git_state'] = report.get('state')
    gate_states = ('validation_failed', 'packaging_failed', 'engine_failed', 'staged_pending_native_acceptance', 'validated_pending_stage', 'installed')
    if (old.get('state') in gate_states and old.get('upstream') == report.get('upstream')
            and old.get('downstream') == report.get('downstream') and report.get('state') == 'up_to_date'):
        report.update({k: old[k] for k in ('state', 'detection_points', 'validation_receipt', 'tests_run',
                       'candidate_repo', 'candidate_sha', 'staged_receipt', 'install_blockers', 'repair_suggestions') if k in old})
    report['schema_version'] = 2
    report['upstream_sha'] = report.get('upstream')
    report['downstream_sha'] = report.get('downstream')
    report['detection_points'] = report.get('detection_points') or [{'check': 'git_merge', 'file': name, 'result': 'failed'}
                                  for name in report.get('conflicts', [])]
    report['repair_suggestions'] = report.get('repair_suggestions') or ([
        'Resolve in an isolated worktree; reuse upstream ownership primitives and preserve downstream environments.',
        'Run Bun, Rust browser/regions, native ownership/lifecycle, migration and rollback fixtures before staging.'
    ] if report.get('state') in ('conflict', 'error', 'local_changes', 'validation_failed', 'packaging_failed', 'engine_failed') else [])
    report['last_handled_event_id'] = old.get('last_handled_event_id')
    report.update(checked_at=dt.datetime.now().astimezone().isoformat(), repo=str(repo),
                  changes_applied=False, application_replaced=False, tests_run=report.get('tests_run', False))
    report['title'], report['summary'] = notification(report)
    report['fingerprint'] = fingerprint(report)
    report['event_id'] = 'ccbaga-' + report['fingerprint'][:24]
    report['last_handled_event_id'] = old.get('last_handled_event_id')
    report['requires_attention'] = report['state'] in ('conflict', 'error', 'local_changes', 'validation_failed', 'packaging_failed', 'engine_failed')
    report['pending_event'] = report['requires_attention'] and report['event_id'] != report['last_handled_event_id']
    report['status_path'] = str(state_file)
    report['notification'] = 'not_needed'
    report['notified_fingerprint'] = old.get('notified_fingerprint')
    atomic_json(state_file, report)
    should_notify = report['state'] in ('conflict', 'error', 'local_changes', 'validation_failed', 'packaging_failed', 'engine_failed') and report['fingerprint'] != old.get('notified_fingerprint')
    if should_notify and notify:
        try:
            detail = report['summary']
            if report.get('conflicts'):
                detail += '\n\n' + '\n'.join(report['conflicts'][:8])
            report['notification'] = show_notice(state_dir, report['title'], detail, state_file)
            if report['notification'] == 'shown':
                report['notified_fingerprint'] = report['fingerprint']
        except (OSError, subprocess.SubprocessError):
            report['notification'] = 'failed'
        atomic_json(state_file, report)
    lock.close()
    return report


def show_notice(state_dir, title, detail, report_path):
    """On-demand only: build a tiny alert, wait for dismissal, then delete all helper files."""
    if sys.platform != 'darwin':
        return 'unavailable'
    with tempfile.TemporaryDirectory(prefix='notice-', dir=state_dir) as temp:
        app = Path(temp) / 'CCBaga Update Notice.app'
        contents = app / 'Contents'
        (contents / 'MacOS').mkdir(parents=True)
        binary = contents / 'MacOS/upstream-notice'
        source = Path(__file__).with_name('upstream_notice.m')
        subprocess.run(['clang', '-fobjc-arc', '-framework', 'AppKit', str(source), '-o', str(binary)],
                       check=True, capture_output=True, timeout=45)
        (contents / 'Info.plist').write_bytes(plistlib.dumps({
            'CFBundleIdentifier':'ai.local.mingle.ccbaga.update-notice',
            'CFBundleName':'CCBaga 更新检查', 'CFBundleExecutable':'upstream-notice',
            'CFBundlePackageType':'APPL', 'LSUIElement':True, 'NSHighResolutionCapable':True}))
        subprocess.run([str(binary), title, detail, str(report_path)], check=True)
    return 'shown'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--ack-event', help='Acknowledge exactly the event already handled by the existing daily task')
    parser.add_argument('--no-fetch', action='store_true')
    parser.add_argument('--no-notify', action='store_true')
    args = parser.parse_args()
    if args.ack_event:
        config_path = args.config.resolve()
        config = json.loads(config_path.read_text())
        state_dir = (config_path.parent / config['state_dir']).resolve()
        with (state_dir / 'update.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            state_file = state_dir / 'status.json'
            report = json.loads(state_file.read_text())
            if report['event_id'] != args.ack_event:
                raise RuntimeError('Event changed; refusing to acknowledge an unseen conflict')
            report['last_handled_event_id'] = args.ack_event
            report['pending_event'] = False
            atomic_json(state_file, report)
        print(json.dumps(report, ensure_ascii=False))
        return 0
    report = check(args.config, not args.no_fetch, not args.no_notify)
    print(json.dumps(report, ensure_ascii=False))
    return 1 if report['state'] in ('conflict', 'error', 'local_changes', 'validation_failed', 'packaging_failed', 'engine_failed') else 0


if __name__ == '__main__':
    sys.exit(main())
