#!/usr/bin/env python3
"""Fail-closed downstream acceptance using synthetic storage and HOME only."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
from check_upstream_update import atomic_json, require, run_git


def verify(repo, output, bun):
    repo, output = Path(repo).resolve(), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    tempbase = repo / '_trash/acceptance'
    tempbase.mkdir(parents=True, exist_ok=True)
    receipt = {'schema_version': 1, 'source_sha': require(run_git(repo, 'rev-parse', 'HEAD')),
               'upstream_sha': require(run_git(repo, 'rev-parse', 'origin/main')),
               'checks': [], 'passed': False, 'synthetic_home': True,
               'native_gui_accepted': False, 'application_replaced': False}
    with tempfile.TemporaryDirectory(dir=tempbase) as temporary:
        tmp = Path(temporary)
        (tmp/'home').mkdir(); (tmp/'tmp').mkdir()
        env = dict(os.environ, HOME=str(tmp/'home'), CARGO_HOME=os.environ.get('CARGO_HOME', str(Path.home()/'.cargo')),
                   RUSTUP_HOME=os.environ.get('RUSTUP_HOME', str(Path.home()/'.rustup')),
                   TMPDIR=str(tmp/'tmp')+'/', CCBAGA_TEST_TMP=str(tmp/'tmp'),
                   TAURI_CONFIG='{"bundle":{"resources":[]}}')
        steps = [('bun-install', [bun, 'install', '--frozen-lockfile']),
                 ('bun-tests', [bun, 'test']), ('bun-build', [bun, 'run', 'build']),
                 ('maintenance-fixtures', ['ai-python', '-m', 'unittest', 'discover', '-s', 'scripts/downstream', '-p', 'test_*.py']),
                 ('rust-browser', ['cargo', 'test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', 'browser::', '--', '--test-threads=1']),
                 ('rust-regions', ['cargo', 'test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', 'regions::', '--', '--test-threads=1'])]
        for name in ['BrowserProfileTests', 'BrowserTests', 'BrowserDiscoveryTests', 'SystemTests']:
            binary = tmp/name
            steps.append((name+'-compile', ['clang', '-fobjc-arc', '-Wall', '-Wextra', str(repo/'src-tauri/native'/(name+'.m')),
                '-framework', 'AppKit', '-framework', 'ApplicationServices', '-framework', 'CoreServices', '-framework', 'CoreLocation',
                '-framework', 'SystemConfiguration', '-framework', 'CoreGraphics', '-o', str(binary)]))
            args = [str(tmp)] if name == 'BrowserDiscoveryTests' else ([] if name == 'SystemTests' else ['--user-data-dir=/test/独立 环境'])
            steps.append((name, [str(binary), *args]))
        steps.append(('diff-check', ['git', 'diff', '--check']))
        for name, command in steps:
            log = output/(name+'.log')
            with log.open('w') as file:
                try:
                    result = subprocess.run(command, cwd=repo, env=env, stdout=file, stderr=subprocess.STDOUT, timeout=1200)
                    code = result.returncode
                except (OSError, subprocess.TimeoutExpired) as error:
                    file.write(str(error)); code = 124
            receipt['checks'].append({'check': name, 'exit_code': code, 'log': str(log)})
            atomic_json(output/'validation.json', receipt)
            print(name, code, flush=True)
            if code:
                return receipt
    receipt['passed'] = True
    atomic_json(output/'validation.json', receipt)
    return receipt


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--bun', required=True)
    args = parser.parse_args()
    raise SystemExit(0 if verify(args.repo, args.output, args.bun)['passed'] else 1)
