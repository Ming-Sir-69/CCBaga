#!/usr/bin/env python3
"""Record reviewed source as a downstream commit without touching the user's HEAD or index."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import subprocess
import uuid
from check_upstream_update import atomic_json, file_digest


def snapshot(config_path, message, include=(), integrate=False):
    config_path = config_path.resolve()
    config = json.loads(config_path.read_text())
    repo = (config_path.parent / config['repo']).resolve()
    state_dir = (config_path.parent / config['state_dir']).resolve()
    state_dir.mkdir(parents=True, exist_ok=True)
    with (state_dir / 'update.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        index = state_dir / ('snapshot-index-' + uuid.uuid4().hex)
        env = dict(os.environ, GIT_INDEX_FILE=str(index), GIT_TERMINAL_PROMPT='0')
        def git(*args, input=None, check=True):
            result = subprocess.run(['git', '-C', str(repo), *args], input=input, env=env,
                                    text=True, capture_output=True, timeout=45)
            if check and result.returncode:
                raise RuntimeError((result.stderr or result.stdout)[:1000])
            return result
        ref = config['downstream_ref']
        if not ref.startswith('refs/heads/downstream/'):
            raise ValueError('Snapshot ref must be under refs/heads/downstream/')
        checked_out = git('worktree', 'list', '--porcelain').stdout.splitlines()
        if 'branch ' + ref in checked_out:
            raise RuntimeError('Refusing to move a branch checked out in a working tree; keep the editing lane separate.')
        previous = git('rev-parse', '--verify', ref, check=False)
        old = previous.stdout.strip() if previous.returncode == 0 else None
        parent = old or git('rev-parse', config.get('base_ref', 'HEAD') + '^{commit}').stdout.strip()
        try:
            git('read-tree', parent)
            git('add', '-u', '--', '.')
            for name in include:
                path = Path(name)
                if path.is_absolute() or '..' in path.parts or '.git' in path.parts or not (repo / path).is_file():
                    raise ValueError('Include must name a reviewed source file inside the repository')
                git('add', '--', name)
            tree = git('write-tree').stdout.strip()
            parents = ['-p', parent]
            if integrate:
                upstream = git('rev-parse', config['remote'] + '/' + config['upstream_branch']).stdout.strip()
                if upstream != parent:
                    parents += ['-p', upstream]
            oid = git('commit-tree', tree, *parents, input=message.strip() + '\n').stdout.strip()
            git('update-ref', ref, oid, old or '0' * 40)
            names = git('ls-tree', '-r', '--name-only', oid).stdout.splitlines()
            manifest = {name: file_digest(repo / name) for name in names}
            atomic_json(config_path.parent / config['manifest'], manifest)
            atomic_json(state_dir / 'checkpoint.json', {'commit':oid, 'tree':tree, 'ref':ref,
                        'integrated_upstream':integrate, 'source_file_count':len(names)})
            return oid
        finally:
            index.unlink(missing_ok=True)
            index.with_name(index.name + '.lock').unlink(missing_ok=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--message', required=True)
    parser.add_argument('--include', action='append', default=[], help='Explicitly include a reviewed new source file')
    parser.add_argument('--integrate-upstream', action='store_true', help='Only after upstream changes have been integrated and tested')
    parser.add_argument('--reviewed', action='store_true', required=True, help='Confirm source, privacy and tests were reviewed')
    args = parser.parse_args()
    print(snapshot(args.config, args.message, args.include, args.integrate_upstream))
