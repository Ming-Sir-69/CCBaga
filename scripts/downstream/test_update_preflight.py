"""Synthetic Git histories only; no network, GUI, real repository changes or browser profiles."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from check_upstream_update import inspect, file_digest, fingerprint, check
from save_downstream_snapshot import snapshot

BASE = Path(__file__).resolve().parents[2] / '_trash/update-preflight-tests'
BASE.mkdir(parents=True, exist_ok=True)


class UpdateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=BASE)
        self.root = Path(self.temp.name)
        self.repo = self.root / 'repo'; self.repo.mkdir()
        self.git('init', '-b', 'main')
        self.git('config', 'user.name', 'Synthetic Tester')
        self.git('config', 'user.email', 'test@example.invalid')
        (self.repo / 'one.txt').write_text('base\n')
        (self.repo / 'two.txt').write_text('base\n')
        self.git('add', '.'); self.git('commit', '-m', 'base')
        self.base = self.git('rev-parse', 'HEAD')
        self.git('checkout', '-b', 'downstream/test')
        (self.repo / 'one.txt').write_text('downstream\n')
        self.git('commit', '-am', 'downstream')
        self.down = self.git('rev-parse', 'HEAD')
        self.git('checkout', '-b', 'upstream-clean', self.base)
        (self.repo / 'two.txt').write_text('upstream\n')
        self.git('commit', '-am', 'upstream clean')
        self.clean = self.git('rev-parse', 'HEAD')
        self.git('checkout', '-b', 'upstream-conflict', self.base)
        (self.repo / 'one.txt').write_text('conflicting upstream\n')
        self.git('commit', '-am', 'upstream conflict')
        self.conflict = self.git('rev-parse', 'HEAD')
        self.git('checkout', 'downstream/test')
        self.git('remote', 'add', 'origin', str(self.repo))
        self.git('update-ref', 'refs/remotes/origin/main', self.base)
        self.config = self.root / 'config.json'
        self.config.write_text(json.dumps({'repo':str(self.repo), 'remote':'origin',
            'expected_remote_url':str(self.repo), 'upstream_branch':'main',
            'downstream_ref':'refs/heads/downstream/test', 'manifest':'manifest.json',
            'state_dir':'state', 'notifier_app':'missing.app'}))
        (self.root / 'manifest.json').write_text(json.dumps({name:file_digest(self.repo/name) for name in ['one.txt','two.txt']}))

    def git(self, *args):
        return subprocess.run(['git','-C',str(self.repo),*args],check=True,text=True,capture_output=True).stdout.strip()

    def tearDown(self):
        self.temp.cleanup()

    def test_clean_and_conflicting_merges_never_change_index_head_or_files(self):
        before = ((self.repo/'.git/index').read_bytes(), self.git('rev-parse','HEAD'), (self.repo/'one.txt').read_bytes())
        clean = inspect(self.repo,self.down,self.clean)
        conflict = inspect(self.repo,self.down,self.conflict)
        self.assertEqual(clean['state'],'merge_clean_pending_tests')
        self.assertEqual(conflict['state'],'conflict')
        self.assertIn('one.txt',conflict['conflicts'])
        self.assertEqual(before,((self.repo/'.git/index').read_bytes(),self.git('rev-parse','HEAD'),(self.repo/'one.txt').read_bytes()))

    def test_merged_upstream_and_local_drift_are_distinct(self):
        self.assertEqual(inspect(self.repo,self.down,self.base)['state'],'up_to_date')
        manifest={'one.txt':file_digest(self.repo/'one.txt')}
        (self.repo/'one.txt').write_text('unrecorded edit\n')
        report=inspect(self.repo,self.down,self.base,manifest)
        self.assertEqual(report['state'],'local_changes')
        self.assertIn('one.txt',report['working_tree_changed'])

    def test_notification_dedup_ignores_timestamp(self):
        a={'state':'conflict','upstream':self.conflict,'downstream':self.down,'conflicts':['one.txt'],'checked_at':'one'}
        self.assertEqual(fingerprint(a),fingerprint(dict(a,checked_at='two')))
        self.assertNotEqual(fingerprint(a),fingerprint(dict(a,upstream=self.clean)))

    def test_clean_update_does_not_display_a_popup(self):
        self.git('update-ref','refs/remotes/origin/main',self.clean)
        with patch('check_upstream_update.show_notice') as notice:
            result=check(self.config,fetch=False,notify=True)
        self.assertEqual(result['state'],'merge_clean_pending_tests')
        self.assertFalse(result['tests_run'])
        notice.assert_not_called()

    def test_bad_remote_stays_error_and_persists_report(self):
        c=json.loads(self.config.read_text());c['expected_remote_url']='different';self.config.write_text(json.dumps(c))
        result=check(self.config,fetch=False,notify=False)
        self.assertEqual(result['state'],'error')
        self.assertFalse(result['application_replaced'])
        self.assertEqual(json.loads((self.root/'state/status.json').read_text())['state'],'error')

    def test_snapshot_rejects_a_checked_out_target(self):
        before=self.git('rev-parse','HEAD')
        with self.assertRaises(RuntimeError):
            snapshot(self.config,'must not move current HEAD')
        self.assertEqual(before,self.git('rev-parse','HEAD'))

    def test_snapshot_preserves_editing_lane_and_records_explicit_new_file(self):
        config=json.loads(self.config.read_text());config['downstream_ref']='refs/heads/downstream/saved';self.config.write_text(json.dumps(config))
        (self.repo/'one.txt').write_text('reviewed change\n')
        (self.repo/'new.txt').write_text('reviewed new file\n')
        before=((self.repo/'.git/index').read_bytes(),self.git('rev-parse','HEAD'),self.git('status','--porcelain'))
        commit=snapshot(self.config,'reviewed snapshot',['new.txt'])
        self.assertEqual(self.git('show',commit+':new.txt'),'reviewed new file')
        self.assertEqual(before,((self.repo/'.git/index').read_bytes(),self.git('rev-parse','HEAD'),self.git('status','--porcelain')))
        self.assertEqual(check(self.config,fetch=False,notify=False)['state'],'up_to_date')


if __name__=='__main__':
    unittest.main()
