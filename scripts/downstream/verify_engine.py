"""Run the candidate native lifecycle bridge against isolated synthetic engine profiles only."""
import argparse
import http.server
import json
import os
from pathlib import Path
import plistlib
import subprocess
import tempfile
import threading
import time
from urllib.parse import urlencode

PROJECT=Path(__file__).resolve().parents[2]


def verify(source, output, engine_app=None):
    source=Path(source).resolve();output=Path(output).resolve();output.mkdir(parents=True,exist_ok=True)
    base=PROJECT/'_trash/engine-integration';base.mkdir(parents=True,exist_ok=True)
    engine_app=Path(engine_app or '/Applications/CCBaga-B.app')
    info=plistlib.loads((engine_app/'Contents/Info.plist').read_bytes())
    assert info['CFBundleIdentifier']=='ai.local.requirement-french.browser'
    engine=engine_app/'Contents/MacOS'/info['CFBundleExecutable']
    page=(PROJECT/'scripts/downstream/fixtures/ccbaga_isolation.html').read_text()
    reports={};children=[]
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self,*args):pass
        def do_GET(self):
            body=page.encode();self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        def do_POST(self):
            length=int(self.headers.get('Content-Length','0'));assert 0<length<10000
            data=json.loads(self.rfile.read(length));reports[data['label']]=data;self.send_response(204);self.end_headers()
    result={'engine':str(engine),'engine_version':info['CFBundleShortVersionString'],'source':str(source),'passed':False,'checks':[],
            'scope':'Candidate native bridge + installed engine, synthetic profiles and localhost page only; not full candidate GUI acceptance.'}
    def check(name,condition):
        result['checks'].append({'check':name,'passed':bool(condition)});assert condition,name
    with tempfile.TemporaryDirectory(dir=base) as temporary:
        tmp=Path(temporary);(tmp/'home').mkdir();(tmp/'tmp').mkdir()
        env=dict(os.environ,HOME=str(tmp/'home'),TMPDIR=str(tmp/'tmp')+'/')
        bridge=tmp/'bridge';driver=tmp/'bridge.m'
        driver.write_text('#import "'+str(source/'src-tauri/native/Browser.m')+'"\n#include <stdio.h>\nint main(int argc,char **argv){@autoreleasepool{if(argc!=3||!strstr(argv[2],"/_trash/engine-integration/"))return 2;char *response=NULL;if(!strcmp(argv[1],"running"))response=rf_browser_running(argv[2]);else if(!strcmp(argv[1],"quit"))response=rf_browser_quit(argv[2]);else if(!strcmp(argv[1],"activity"))response=rf_browser_activity(argv[2]);else return 2;puts(response);free(response);}return 0;}\n')
        with (output/'engine-compile.log').open('w') as log:
            subprocess.run(['clang','-fobjc-arc',str(driver),'-framework','AppKit','-framework','CoreServices','-framework','CoreLocation','-framework','SystemConfiguration','-framework','CoreGraphics','-o',str(bridge)],check=True,stdout=log,stderr=subprocess.STDOUT)
        def native(action,profile):
            return json.loads(subprocess.check_output([str(bridge),action,str(profile)],env=env,text=True))
        with http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler) as server:
            thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
            def start(profile,label,value='',language='en-US',timezone='America/Los_Angeles'):
                profile.mkdir(exist_ok=True)
                url=f'http://127.0.0.1:{server.server_port}/?'+urlencode({'label':label,'write':value})
                args=[str(engine),f'--user-data-dir={profile}','--no-first-run','--no-default-browser-check','--disable-background-networking',
                      '--disable-component-update','--disable-sync','--disable-default-apps','--password-store=basic','--use-mock-keychain',
                      f'--lang={language}',f'--timezone={timezone}',url]
                log=(output/(label+'-engine.log')).open('w')
                child=subprocess.Popen(args,env=env,stdout=log,stderr=subprocess.STDOUT);children.append((child,log,profile));return child
            def wait(label):
                deadline=time.monotonic()+30
                while time.monotonic()<deadline:
                    if label in reports:return reports[label]
                    time.sleep(.1)
                raise RuntimeError('No synthetic page response for '+label)
            def matches(data,value):return data==dict(cookie='isolation='+value,localStorage=value,indexedDB=value,cacheStorage=value)
            a,b,foreign=tmp/'A',tmp/'B',tmp/'foreign';foreign.mkdir()
            try:
                ca=start(a,'A','A');cb=start(b,'B','B','ja-JP','Asia/Tokyo')
                ra,rb=wait('A'),wait('B')
                empty=dict(cookie='',localStorage=None,indexedDB=None,cacheStorage=None)
                check('fresh_A_and_B_have_no_shared_storage',ra['before']==empty and rb['before']==empty)
                check('four_storage_types_are_isolated',matches(ra['current'],'A') and matches(rb['current'],'B'))
                check('official_LA_and_Tokyo_timezones_are_distinct',ra['timezone']=='America/Los_Angeles' and rb['timezone']=='Asia/Tokyo')
                check('candidate_native_ownership_recognizes_both_profiles',native('running',a).get('running') is True and native('running',b).get('running') is True)
                os.symlink(os.readlink(a/'SingletonLock'),foreign/'SingletonLock')
                check('foreign_lock_never_claims_A',native('running',foreign).get('running') is False)
                native('quit',foreign)
                check('foreign_quit_preserves_A_and_B',native('running',a).get('running') is True and native('running',b).get('running') is True)
                (foreign/'SingletonLock').unlink();(foreign/'SingletonLock').write_text('not a symlink')
                check('malformed_lock_blocks_destructive_action','error' in native('quit',foreign))
                native('quit',a);ca.wait(timeout=20)
                check('closing_A_preserves_B',native('running',b).get('running') is True)
                start(a,'A-restart');restarted=wait('A-restart')
                check('A_storage_survives_restart',matches(restarted['before'],'A'))
                check('B_storage_stays_B',matches(reports['B']['current'],'B'))
                result['passed']=True
            except Exception as error:
                result['error']=str(error)
            finally:
                for child,log,profile in children:
                    if child.poll() is None:
                        try:native('quit',profile);child.wait(timeout=15)
                        except Exception:
                            child.terminate()
                            try:child.wait(timeout=5)
                            except subprocess.TimeoutExpired:child.kill();child.wait()
                    log.close()
                server.shutdown()
    (output/'engine-validation.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(result,ensure_ascii=False))
    return result


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--source',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--engine-app',type=Path);args=parser.parse_args()
    raise SystemExit(0 if verify(args.source,args.output,args.engine_app)['passed'] else 1)
