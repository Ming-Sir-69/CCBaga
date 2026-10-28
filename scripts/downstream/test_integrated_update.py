"""Synthetic roots/apps only: one event pipeline, no profile migration, durable pair rollback."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import plistlib
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import integrated_update as hook

spec=importlib.util.spec_from_file_location('integrated_one_shot',Path(__file__).with_name('one_shot_updater.py'))
auto=importlib.util.module_from_spec(spec);spec.loader.exec_module(auto)
BASE=Path(__file__).resolve().parents[2]/'_trash/integrated-tests';BASE.mkdir(parents=True,exist_ok=True)

class IntegratedTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(dir=BASE);self.root=Path(self.temp.name);self.cc=self.root/'CCBaga';self.cc.mkdir();self.apps=self.root/'Applications';self.apps.mkdir();self.data=self.root/'data';self.data.mkdir();self.candidates=self.root/'candidate';self.candidates.mkdir()
        self.old={'active_channel':'official','base_release':'v0.2.49','local_version':'0.2.49'}
        auto.atomic(self.cc/'current-version.json',self.old);auto.atomic(self.cc/'downstream-update.json',{'single_product':True})
        for place,label in ((self.apps,'old'),(self.candidates,'new')):
            for name in auto.OFFICIAL:
                path=place/name;(path/'Contents').mkdir(parents=True);(path/'Contents/Info.plist').write_bytes(plistlib.dumps({'CFBundleIdentifier':auto.IDS[name],'CFBundleShortVersionString':'0.2.50'}));(path/'payload').write_text(label)
        self.profile=self.data/'real-like-profile';self.profile.mkdir();(self.profile/'Cookies').write_text('SYNTHETIC')
        def verify(path,identifier):
            info=plistlib.loads((path/'Contents/Info.plist').read_bytes());assert info['CFBundleIdentifier']==identifier;return info
        self.manager=auto.Installation(root=self.root,apps=self.apps,data=self.data,state_dir=self.root/'state',verifier=verify)
        self.release={'tag_name':'v0.2.50','assets':[{'name':'CCBaga-0.2.50-macos-arm64.dmg','digest':'sha256:fixture','size':10}]}
    def tearDown(self):self.temp.cleanup()
    def test_no_release_update_never_checks_overlay_downloads_or_launches(self):
        release=dict(self.release,tag_name='v0.2.49')
        with patch.object(auto,'github',return_value=release),patch.object(hook,'apply') as apply,patch.object(auto,'candidate_sources') as sources,patch.object(auto,'reap_update_mounts') as reap:
            self.assertEqual(auto.execute(self.manager)['status'],'up-to-date')
        apply.assert_not_called();sources.assert_not_called();reap.assert_not_called()
    def test_new_release_must_use_integrated_hook_and_never_plain_official_apply(self):
        with patch.object(auto,'github',return_value=self.release),patch.object(auto,'running',return_value=False),patch.object(hook,'apply',return_value={'status':'fixture-event'}) as apply,patch.object(auto,'apply_release') as official:
            self.assertEqual(auto.execute(self.manager)['status'],'fixture-event')
        apply.assert_called_once();official.assert_not_called()
    def test_running_pair_defers_without_build_or_profile_mutation(self):
        with patch.object(auto,'github',return_value=self.release),patch.object(auto,'running',return_value=True),patch.object(hook,'apply') as apply:
            self.assertEqual(auto.execute(self.manager)['status'],'deferred-app-running')
        apply.assert_not_called();self.assertEqual((self.profile/'Cookies').read_text(),'SYNTHETIC')
    def test_committed_integrated_pair_retains_original_pair_and_exact_data(self):
        pair={'local_version':'0.2.50-multi.17','source':str(self.root/'active'),'source_commit':'a'*40,'release_source_sha':'b'*40,'asset_fingerprint':'fixture'}
        with patch.object(auto,'running',return_value=False):self.manager.install(self.candidates,dict(self.release,_downstream=pair))
        state=json.loads(self.manager.state_file.read_text());self.assertEqual(state['active_channel'],'self-maintained-integrated')
        rollback=list((self.cc/'operations/rollback').glob('*/old/CCBaga-A.app.saved'));self.assertEqual(len(rollback),1);self.assertEqual((rollback[0]/'payload').read_text(),'old')
        self.assertEqual((self.apps/'CCBaga-A.app/payload').read_text(),'new');self.assertEqual((self.profile/'Cookies').read_text(),'SYNTHETIC')
        self.assertFalse(self.manager.journal.exists())
    def test_partial_integrated_pair_failure_restores_both_originals(self):
        self.manager.checkpoint=lambda phase: (_ for _ in ()).throw(RuntimeError('fixture')) if phase=='new-apps-moved' else None
        with patch.object(auto,'running',return_value=False):
            with self.assertRaises(RuntimeError):self.manager.install(self.candidates,dict(self.release,_downstream={'fixture':True}))
        for name in auto.OFFICIAL:self.assertEqual((self.apps/name/'payload').read_text(),'old')
        self.assertEqual(json.loads(self.manager.state_file.read_text()),self.old)
        self.assertEqual((self.profile/'Cookies').read_text(),'SYNTHETIC')
    def test_same_failed_release_is_cached_without_rechecking_overlay(self):
        status=self.cc/'operations/upstream-update/status.json';auto.atomic(status,{'status':'blocked-integrated-update','asset_fingerprint':hook.asset_key(self.release),'event_id':'fixture-event'})
        with patch.object(auto,'github',return_value=self.release),patch.object(hook,'apply') as apply:
            self.assertEqual(auto.execute(self.manager)['status'],'blocked-update-cached')
        apply.assert_not_called()
    def test_events_notify_once_and_keep_sha_digest_receipt(self):
        api=SimpleNamespace(now=lambda:'fixture-time')
        event={'status':'blocked-integrated-update','release_tag':'v0.2.50','upstream_sha':'b'*40,'downstream_sha':'a'*40,'asset_fingerprint':'digest','detection_points':[{'file':'fixture.rs'}]}
        with patch.object(hook,'notice',return_value='fixture') as notice:
            a=hook.persist_event(self.cc,api,dict(event));b=hook.persist_event(self.cc,api,dict(event))
        self.assertEqual(a['event_id'],b['event_id']);notice.assert_called_once()
    def test_pair_manifest_rejects_another_B_payload(self):
        a=self.candidates/'CCBaga-A.app/Contents/Resources';a.mkdir()
        pair={'local_version':'0.2.50','engine_bundle_sha256':hook.digest_tree(self.candidates/'CCBaga-B.app')}
        auto.atomic(a/'integration-pair.json',pair);hook.verify_pair(self.candidates)
        (self.candidates/'CCBaga-B.app/payload').write_text('wrong-engine')
        with self.assertRaisesRegex(RuntimeError,'fingerprint'):hook.verify_pair(self.candidates)

    def test_retained_pair_rollback_restores_exact_version_state(self):
        pair={'local_version':'0.2.50-multi.17','source':str(self.root/'active'),'source_commit':'a'*40,'release_source_sha':'b'*40,'asset_fingerprint':'fixture'}
        with patch.object(auto,'running',return_value=False):
            self.manager.install(self.candidates,dict(self.release,_downstream=pair))
            retained=next((self.cc/'operations/rollback').iterdir())
            result=auto.restore_retained(self.manager,retained)
        self.assertEqual(result['status'],'restored-retained-pair')
        self.assertEqual(json.loads(self.manager.state_file.read_text()),self.old)
        for name in auto.OFFICIAL:self.assertEqual((self.apps/name/'payload').read_text(),'old')
        self.assertEqual((self.profile/'Cookies').read_text(),'SYNTHETIC')
