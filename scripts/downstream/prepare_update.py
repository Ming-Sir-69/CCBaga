#!/usr/bin/env python3
"""Mandatory source-update gate; create isolated merge candidates and verify before packaging."""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
from check_upstream_update import check, atomic_json, require, run_git
from verify_candidate import verify


def prepare(config_path, bun):
    config_path = Path(config_path).resolve()
    config = json.loads(config_path.read_text())
    repo = (config_path.parent/config['repo']).resolve()
    state_dir = (config_path.parent/config['state_dir']).resolve()
    state_dir.mkdir(parents=True, exist_ok=True)
    with (state_dir/'prepare.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        report = check(config_path, fetch=True, notify=False)
        if report['state'] not in ('up_to_date', 'merge_clean_pending_tests'):
            return report
        if report['state'] == 'merge_clean_pending_tests':
            candidates = (config_path.parent/config.get('candidates_dir', 'candidates')).resolve()
            candidates.mkdir(parents=True, exist_ok=True)
            candidate = candidates/(report['upstream'][:12]+'-'+report['downstream'][:12])
            if not candidate.exists():
                require(run_git(repo, 'worktree', 'add', '--detach', str(candidate), report['downstream']))
                result = run_git(candidate, 'merge', '--no-ff', '--no-edit', report['upstream'], timeout=120)
                if result.returncode:
                    report.update(state='conflict', error=result.stderr or result.stdout)
                    atomic_json(state_dir/'status.json', report)
                    return report
            # Reject cached candidates altered since the tested two-parent merge.
            parents = require(run_git(candidate, 'show', '-s', '--format=%P', 'HEAD')).split()
            if parents != [report['downstream'], report['upstream']]:
                raise RuntimeError('Candidate ancestry changed; retained for inspection')
        else:
            candidate = repo
            if require(run_git(candidate, 'rev-parse', 'HEAD')) != report['downstream']:
                raise RuntimeError('Build source HEAD must equal the saved downstream commit')
        if require(run_git(candidate, 'status', '--porcelain')):
            raise RuntimeError('Candidate has unreviewed source changes')
        output = state_dir/'validation'/require(run_git(candidate, 'rev-parse', 'HEAD'))
        receipt = verify(candidate, output, bun)
        report.update(state='validated_pending_stage' if receipt['passed'] else 'validation_failed',
                      tests_run=True, candidate_repo=str(candidate), candidate_sha=receipt['source_sha'],
                      validation_receipt=str(output/'validation.json'),
                      detection_points=[x for x in receipt['checks'] if x['exit_code']],
                      repair_suggestions=[] if receipt['passed'] else ['Inspect failed check logs; repair only in the isolated candidate, then retry.'],
                      requires_attention=not receipt['passed'])
        report['fingerprint'] = hashlib.sha256(json.dumps({k:report.get(k) for k in ('state','upstream','downstream','detection_points')},sort_keys=True).encode()).hexdigest()
        report['event_id'] = 'ccbaga-'+report['fingerprint'][:24]
        report['pending_event'] = report['requires_attention'] and report['event_id'] != report.get('last_handled_event_id')
        atomic_json(state_dir/'status.json', report)
        return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--bun', required=True)
    args = parser.parse_args()
    try:
        result = prepare(args.config, args.bun)
    except (OSError, ValueError, RuntimeError) as error:
        config_path = args.config.resolve()
        config = json.loads(config_path.read_text())
        state_file = (config_path.parent/config['state_dir']).resolve()/'status.json'
        result = json.loads(state_file.read_text()) if state_file.exists() else {}
        result.update(state='error', error=str(error)[:1000], application_replaced=False,
                      detection_points=[{'check':'prepare_gate','result':'failed'}],
                      repair_suggestions=['Inspect the isolated candidate and retry; installed app is retained.'],
                      requires_attention=True)
        result['event_id'] = 'ccbaga-'+hashlib.sha256(json.dumps({k:result.get(k) for k in ('state','upstream','downstream','error')},sort_keys=True).encode()).hexdigest()[:24]
        result['pending_event'] = result['event_id'] != result.get('last_handled_event_id')
        atomic_json(state_file, result)
    print(json.dumps(result, ensure_ascii=False))
    raise SystemExit(0 if result['state'] == 'validated_pending_stage' else 1)
