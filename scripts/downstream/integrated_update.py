"""Event-driven build hook for the existing CCBaga one-shot updater and pair transaction."""
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile
from types import SimpleNamespace
from check_upstream_update import atomic_json, require, run_git
from prepare_update import prepare
from verify_engine import verify as verify_engine

A_ID = 'ai.local.requirement-french'
B_ID = A_ID + '.browser'


def digest_tree(root):
    root = Path(root)
    digest = hashlib.sha256()
    for path in sorted(root.rglob('*')):
        name = path.relative_to(root).as_posix()
        if path.is_symlink():
            value = 'link:' + os.readlink(path)
        elif path.is_file():
            with path.open('rb') as file:
                value = hashlib.file_digest(file, 'sha256').hexdigest()
        else:
            continue
        digest.update((name+'\0'+value+'\n').encode())
    return digest.hexdigest()


def asset_key(release):
    assets = [(a['name'], a.get('digest'), a.get('size')) for a in release.get('assets', []) if a['name'].endswith('macos-arm64.dmg')]
    return hashlib.sha256(json.dumps([release['tag_name'], assets], sort_keys=True).encode()).hexdigest()


def verify_pair(folder, expected=None):
    folder=Path(folder)
    info=plistlib.loads((folder/'CCBaga-A.app/Contents/Info.plist').read_bytes())
    pair=json.loads((folder/'CCBaga-A.app/Contents/Resources/integration-pair.json').read_text())
    binfo=plistlib.loads((folder/'CCBaga-B.app/Contents/Info.plist').read_bytes())
    if info.get('CFBundleIdentifier')!=A_ID or binfo.get('CFBundleIdentifier')!=B_ID:
        raise RuntimeError('Integrated pair identity mismatch')
    if info['CFBundleShortVersionString']!=pair['local_version'] or digest_tree(folder/'CCBaga-B.app')!=pair['engine_bundle_sha256']:
        raise RuntimeError('A+B pair fingerprint/version mismatch')
    if expected and any(pair.get(k)!=expected.get(k) for k in ('source_commit','release_tag','asset_fingerprint')):
        raise RuntimeError('A+B pair does not match the reviewed build receipt')
    return pair


def notice(api, event):
    title='CCBaga 更新完成' if event.get('installed') else 'CCBaga 更新事件'
    detail=event.get('status','unknown')+' · '+event.get('release_tag','')
    try:
        subprocess.run(['/usr/bin/osascript','-e','display notification '+json.dumps(detail,ensure_ascii=False)+' with title '+json.dumps(title,ensure_ascii=False)],capture_output=True,timeout=10)
        return 'notification_requested'
    except (OSError,subprocess.SubprocessError):
        return 'unavailable'


def persist_event(root, api, event):
    # Volatile timestamps and compiler prose never cause repeated notifications.
    fields={k:event.get(k) for k in ('status','release_tag','upstream_sha','downstream_sha','asset_fingerprint','detection_points')}
    event['event_id']='ccbaga-'+hashlib.sha256(json.dumps(fields,sort_keys=True).encode()).hexdigest()[:24]
    folder=Path(root)/'operations/update-events';folder.mkdir(parents=True,exist_ok=True)
    path=folder/(event['event_id']+'.json')
    if not path.exists():
        event['checked_at']=api.now()
        event['notification']=notice(api,event)
        atomic_json(path,event)
    atomic_json(Path(root)/'operations/upstream-update/status.json',event)
    return event


def official_pair(root, release, api):
    root=Path(root)
    # Reuse only a fully verified, matching official-release cache, never an arbitrary installed B.
    prepared=root/'operations/20261028-unified/official-pair-verification.json'
    if prepared.exists():
        receipt=json.loads(prepared.read_text())
        asset=next(a['digest'] for a in release['assets'] if a['name'].endswith('macos-arm64.dmg'))
        if receipt['release']==release['tag_name'] and receipt['digest']==asset:
            folder=Path(receipt['pair'])
            for name in api.OFFICIAL:api.verify(folder/name,api.IDS[name])
            return folder
    folder=root/'_trash/release-cache'/asset_key(release)
    pair=folder/'verified-pair'
    folder.mkdir(parents=True,exist_ok=True)
    if not pair.exists():
        image=api.download_asset(release,folder)
        device=api.attach_image(image,folder/'volume')
        try:
            for name in api.OFFICIAL:api.verify(folder/'volume'/name,api.IDS[name])
            pair.mkdir()
            for name in api.OFFICIAL:api.command(['/usr/bin/ditto',folder/'volume'/name,pair/name],180)
        finally:api.detach_image(device)
    for name in api.OFFICIAL:api.verify(pair/name,api.IDS[name])
    ainfo=plistlib.loads((pair/'CCBaga-A.app/Contents/Info.plist').read_bytes())
    if ainfo['CFBundleShortVersionString']!=release['tag_name'].lstrip('v'):
        raise RuntimeError('Official engine cache baseline mismatch')
    return pair


def build_pair(root, config_path, release, api):
    root=Path(root);config_path=Path(config_path)
    config=json.loads(config_path.read_text());repo=(config_path.parent/config['repo']).resolve()
    # Fetch and freeze the actual release commit only for a real update/manual integration event.
    require(run_git(repo,'fetch','--no-tags',config['remote'],'refs/tags/'+release['tag_name'],timeout=90))
    upstream=require(run_git(repo,'rev-parse','FETCH_HEAD^{commit}'))
    work_config=dict(config,upstream_ref=upstream)
    session=root/'_trash/integrated-build';session.mkdir(parents=True,exist_ok=True)
    frozen=session/'config.json'
    for name in ('repo','manifest','state_dir','candidates_dir'):
        if name in work_config:work_config[name]=str((config_path.parent/work_config[name]).resolve())
    atomic_json(frozen,work_config)
    with (session/'preflight.log').open('w') as log,contextlib.redirect_stdout(log):
        report=prepare(frozen,config['bun'])
    if report['state']!='validated_pending_stage':
        raise RuntimeError('Integrated source gate failed: '+report['state'])
    source=Path(report['candidate_repo']);sha=report['candidate_sha']
    official=official_pair(root,release,api)
    logs=root/'operations/upstream-update/validation'/sha/'integrated';logs.mkdir(parents=True,exist_ok=True)
    with (logs/'engine.log').open('w') as log,contextlib.redirect_stdout(log):
        engine=verify_engine(source,logs/'engine',official/'CCBaga-B.app')
    if not engine['passed']:raise RuntimeError('Candidate engine/profile lifecycle gate failed')
    local=release['tag_name'].lstrip('v')+'-multi.'+str(config['overlay_revision'])
    tauri={'productName':'CCBaga-A','identifier':A_ID,'version':local,'bundle':{'resources':[]},'plugins':{'updater':{'endpoints':[]}}}
    home=session/'home';home.mkdir(exist_ok=True)
    env={k:v for k,v in os.environ.items() if not k.startswith(('APPLE_','TAURI_SIGNING_PRIVATE_KEY'))}
    env.update(HOME=str(home),CARGO_HOME=os.environ.get('CARGO_HOME',str(Path.home()/'.cargo')),RUSTUP_HOME=os.environ.get('RUSTUP_HOME',str(Path.home()/'.rustup')),TAURI_CONFIG=json.dumps(tauri))
    with (logs/'cargo-build.log').open('w') as log:
        subprocess.run(['cargo','build','--locked','--manifest-path','src-tauri/Cargo.toml','--features','custom-protocol,mingle-integration'],cwd=source,env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=1200)
    if require(run_git(source,'status','--porcelain')):raise RuntimeError('Source drift during integrated build')
    stage=session/('pair-'+sha[:12])
    if stage.exists():shutil.rmtree(stage)
    contents=stage/'CCBaga-A.app/Contents';(contents/'MacOS').mkdir(parents=True);(contents/'Resources').mkdir()
    shutil.copy2(source/'src-tauri/target/debug/requirement-french',contents/'MacOS/requirement-french')
    shutil.copytree(source/'dist',contents/'Resources/browser-ui');shutil.copy2(source/'src-tauri/icons/icon.icns',contents/'Resources/icon.icns')
    shutil.copytree(source/'src-tauri/third-party',contents/'Resources/ThirdPartyNotices')
    api.command(['/usr/bin/ditto',official/'CCBaga-B.app',stage/'CCBaga-B.app'],180)
    pair={'schema':1,'source_commit':sha,'source':str(repo),'release_tag':release['tag_name'],'release_source_sha':upstream,
          'upstream_head':require(run_git(repo,'rev-parse','origin/main')),'asset_fingerprint':asset_key(release),'local_version':local,
          'engine_bundle_sha256':digest_tree(stage/'CCBaga-B.app'),'engine_version':plistlib.loads((stage/'CCBaga-B.app/Contents/Info.plist').read_bytes())['CFBundleShortVersionString'],
          'A_signature':'self-maintained ad-hoc; no maintainer signature/notarization','B_signature':'unmodified verified official release engine',
          'gui_acceptance':'waived_by_user_20261028','profile_policy':'official identity/native root unchanged; standalone catalog retained in place; no profile copies/moves',
          'features':['mingle-integration','custom-protocol'],'browser_autostart':False,'validation_receipt':report['validation_receipt']}
    atomic_json(contents/'Resources/integration-pair.json',pair)
    info={'CFBundleName':'CCBaga-A','CFBundleDisplayName':'CCBaga-A','CFBundleIdentifier':A_ID,'CFBundleExecutable':'requirement-french','CFBundlePackageType':'APPL',
          'CFBundleShortVersionString':local,'CFBundleVersion':release['tag_name'].lstrip('v')+'.'+str(config['overlay_revision']),'CFBundleIconFile':'icon.icns','NSHighResolutionCapable':True,
          'LSMinimumSystemVersion':'12.0','NSPrincipalClass':'NSApplication','MingleSelfMaintained':True}
    (contents/'Info.plist').write_bytes(plistlib.dumps(info))
    api.command(['/usr/bin/codesign','--force','--deep','--sign','-',stage/'CCBaga-A.app'],120)
    for name in api.OFFICIAL:api.verify(stage/name,api.IDS[name])
    verify_pair(stage,pair)
    atomic_json(root/'operations/upstream-update/staged-pair.json',dict(pair,staged_pair=str(stage)))
    return stage,pair


def converge_source(config_path, pair):
    """Fast-forward only our clean active worktree; no second daily source survives a successful update."""
    config_path=Path(config_path).resolve();config=json.loads(config_path.read_text())
    repo=(config_path.parent/config['repo']).resolve()
    if require(run_git(repo,'status','--porcelain')):raise RuntimeError('Active source has unreviewed changes')
    current=require(run_git(repo,'rev-parse','HEAD'))
    if current!=pair['source_commit']:
        require(run_git(repo,'merge-base','--is-ancestor',current,pair['source_commit']))
        require(run_git(repo,'merge','--ff-only',pair['source_commit']))
    names=require(run_git(repo,'ls-tree','-r','--name-only','HEAD')).splitlines()
    from check_upstream_update import file_digest
    atomic_json(config_path.parent/config['manifest'],{name:file_digest(repo/name) for name in names})
    checked=require(run_git(repo,'symbolic-ref','HEAD'))
    if checked!=config['downstream_ref']:
        from save_downstream_snapshot import snapshot
        snapshot(config_path,'Reviewed integrated pair source',source_commit=pair['source_commit'])
    return str(repo)


def apply(manager, release, api_dict, stage_only=False):
    api=SimpleNamespace(**api_dict);root=manager.root/'CCBaga'
    config=root/'downstream-update.json'
    event={'status':'preparing','release_tag':release['tag_name'],'asset_fingerprint':asset_key(release),'installed':False,'application_replaced':False,
           'gui_acceptance':'waived_by_user_20261028','detection_points':[]}
    try:
        folder,pair=build_pair(root,config,release,api)
        event.update(upstream_sha=pair['release_source_sha'],downstream_sha=pair['source_commit'],staged_pair=str(folder))
        if stage_only:return persist_event(root,api,dict(event,status='new-integrated-pair-ready'))
        if api.running(manager.apps):return persist_event(root,api,dict(event,status='deferred-app-running'))
        converge_source(config,pair)
        manager.install(folder,dict(release,_downstream=pair))
        verify_pair(manager.apps,pair)
        return persist_event(root,api,dict(event,status='updated-integrated',installed=True,application_replaced=True))
    except Exception as error:
        status=root/'operations/upstream-update/status.json'
        previous=json.loads(status.read_text()) if status.exists() else {}
        event.update(status='blocked-integrated-update',error=str(error)[:1000],
                     upstream_sha=previous.get('upstream_sha'),downstream_sha=previous.get('downstream_sha'),
                     detection_points=previous.get('detection_points') or [{'check':'integrated_build_install','result':'failed'}],
                     repair_suggestions=['Inspect preflight/build logs, repair only the active downstream branch, rerun the same manual update entry; installed pair remains protected.'])
        return persist_event(root,api,event)
