import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from test_update_preflight import UpdateTests
from check_upstream_update import check, file_digest
from install_staged_app import install


class GateTests(UpdateTests):
    def test_persistent_event_and_ack_survive_repeated_checks(self):
        self.git('update-ref', 'refs/remotes/origin/main', self.conflict)
        a = check(self.config, fetch=False, notify=False)
        self.assertTrue(a['pending_event'])
        path = self.root/'state/status.json'
        a['last_handled_event_id'] = a['event_id']; path.write_text(json.dumps(a))
        b = check(self.config, fetch=False, notify=False)
        self.assertEqual(a['event_id'], b['event_id'])
        self.assertFalse(b['pending_event'])
        self.assertEqual(b['upstream_sha'], self.conflict)
        self.assertEqual(b['detection_points'][0]['file'], 'one.txt')

    def test_daily_git_check_preserves_failed_semantic_gate(self):
        a=check(self.config,fetch=False,notify=False)
        a.update(state='validation_failed',tests_run=True,detection_points=[{'check':'api_contract','exit_code':1}],validation_receipt='fixture.json')
        (self.root/'state/status.json').write_text(json.dumps(a))
        b=check(self.config,fetch=False,notify=False)
        self.assertEqual(b['git_state'],'up_to_date')
        self.assertEqual(b['state'],'validation_failed')
        self.assertTrue(b['pending_event']);self.assertTrue(b['tests_run'])
        self.assertEqual(b['detection_points'][0]['check'],'api_contract')

    def test_bad_new_upstream_stops_before_build(self):
        from prepare_update import prepare
        self.git('update-ref', 'refs/remotes/origin/main', self.conflict)
        with patch('prepare_update.check', side_effect=lambda *a, **k: check(self.config, fetch=False, notify=False)), patch('prepare_update.verify') as verify:
            report = prepare(self.config, 'never-executed-bun')
        self.assertEqual(report['state'], 'conflict'); verify.assert_not_called()


class RollbackTests(unittest.TestCase):
    def test_new_app_failure_restores_old_binary_without_touching_profile(self):
        base = Path(__file__).resolve().parents[2]/'_trash/rollback-tests'; base.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=base) as temporary:
            root = Path(temporary); old = root/'CCBaga 多环境.app'; new = root/'candidate.app'
            for app, data in [(old, b'old'), (new, b'new')]:
                (app/'Contents/MacOS').mkdir(parents=True); (app/'Contents/MacOS/requirement-french').write_bytes(data)
            profile = root/'synthetic-profile'; profile.mkdir(); (profile/'Cookies').write_text('fixture')
            def fail(phase):
                if phase == 'new_installed': raise RuntimeError('injected post-install failure')
            receipt = {'native_gui_accepted': True, 'binary_sha256': file_digest(new/'Contents/MacOS/requirement-french')}
            with self.assertRaises(RuntimeError):
                install(new, old, root/'state', receipt, verifier=lambda _: None, running=lambda _: False, checkpoint=fail)
            self.assertEqual((old/'Contents/MacOS/requirement-french').read_bytes(), b'old')
            self.assertEqual((profile/'Cookies').read_text(), 'fixture')
            self.assertEqual(json.loads((root/'state/install-transaction.json').read_text())['phase'], 'rolled_back')

    def test_running_or_unaccepted_candidate_is_never_replaced(self):
        base = Path(__file__).resolve().parents[2]/'_trash/rollback-tests'; base.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=base) as temporary:
            root=Path(temporary); old=root/'CCBaga 多环境.app'; new=root/'candidate.app'
            for app in (old,new):
                (app/'Contents/MacOS').mkdir(parents=True); (app/'Contents/MacOS/requirement-french').write_bytes(b'fixture')
            for receipt, running in [({},False),({'native_gui_accepted':True,'binary_sha256':file_digest(new/'Contents/MacOS/requirement-french')},True)]:
                with self.assertRaises(RuntimeError):
                    install(new,old,root/'state',receipt,verifier=lambda _:None,running=lambda _:running)
            self.assertFalse((root/'state/install-transaction.json').exists())

    def test_interrupted_rename_is_recovered_from_durable_journal(self):
        from install_staged_app import recover
        base=Path(__file__).resolve().parents[2]/'_trash/rollback-tests';base.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(dir=base) as temporary:
            root=Path(temporary);destination=root/'CCBaga 多环境.app';stage=root/'.mingle-ccbaga-fixture';state=root/'state'
            (stage/'old.app').mkdir(parents=True);(stage/'old.app/binary').write_text('old');state.mkdir()
            (state/'install-transaction.json').write_text(json.dumps({'schema':1,'destination':str(destination),'stage':str(stage),'phase':'prepared'}))
            report=recover(state,destination,verifier=lambda _:None,running=lambda _:False)
            self.assertEqual((destination/'binary').read_text(),'old');self.assertEqual(report['phase'],'rolled_back')
