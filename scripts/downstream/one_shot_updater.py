"""One-shot CCBaga update worker. No UI, daemon, AI agent, or Claude installer.

Only a newer signed official A+B release passing the pinned compatibility gate
can replace the current pair. An incompatible release leaves the working DEV.
"""
from pathlib import Path
import argparse
import base64
import datetime
import fcntl
import hashlib
import importlib.util
import json
import os
import plistlib
import re
import shutil
import signal
import subprocess
import tempfile
import sys
import urllib.request
import urllib.parse

ROOT = Path(__file__).resolve().parent.parent if Path(__file__).name == "auto_update.py" else Path(__file__).resolve().parents[4]
IDS = {"CCBaga-A.app": "ai.local.requirement-french", "CCBaga-B.app": "ai.local.requirement-french.browser",
       "CCBaga-TW-Dev-A.app": "ai.local.ccbaga-tw-dev", "CCBaga-TW-Dev-B.app": "ai.local.ccbaga-tw-dev.browser"}
OFFICIAL = ("CCBaga-A.app", "CCBaga-B.app")
TEAM = "6UB6WM6VQR"
PIN = "da54dd7191a92ab70b606017916ca2fd73486f3d"
GH = "/opt/homebrew/bin/gh"
REGISTER = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"


def now():
    return datetime.datetime.now().astimezone().isoformat()


def command(args, timeout=60, env=None):
    result = subprocess.run([str(x) for x in args], capture_output=True, text=True, timeout=timeout, env=env)
    if result.returncode:
        # A short diagnostic only; never log environment, auth tokens, profile contents or API bodies.
        raise RuntimeError(str(args[0]) + " failed: " + result.stderr.strip()[-700:])
    return result.stdout


def atomic(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, prefix=".pending-", delete=False) as file:
        pending = Path(file.name)
        try:
            os.chmod(pending, 0o600)
            json.dump(value, file, ensure_ascii=False, indent=2)
            file.write("\n")
            file.flush()
            os.fsync(file.fileno())
            os.replace(pending, path)
            descriptor = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(descriptor)
            finally:
                os.close(descriptor)
        finally:
            pending.unlink(missing_ok=True)


def github(endpoint):
    return json.loads(command([GH, "api", "repos/liseami/CCBaga/" + endpoint], 50))


def version(tag):
    match = re.fullmatch(r"v?(\d+)\.(\d+)\.(\d+)", tag)
    if not match:
        raise ValueError("Unrecognized stable release version")
    return tuple(map(int, match.groups()))


def running(apps_dir):
    rows = command(["/bin/ps", "-ww", "-axo", "uid=,pid=,comm="])
    prefixes = tuple(str(apps_dir / name) + "/" for name in IDS)
    if apps_dir == Path("/Applications"):
        prefixes += (str(Path.home()/"Applications/CCBaga 多环境.app")+"/",)
    for line in rows.splitlines():
        fields = line.strip().split(None, 2)
        if len(fields) == 3 and fields[0] == str(os.getuid()) and fields[2].startswith(prefixes):
            return True
    return False


def verify(app, expected_id):
    if app.is_symlink() or not app.is_dir():
        raise RuntimeError("Invalid app bundle")
    with (app / "Contents/Info.plist").open("rb") as file:
        info = plistlib.load(file)
    if info.get("CFBundleIdentifier") != expected_id:
        raise RuntimeError("App identity mismatch")
    command(["/usr/bin/codesign", "--verify", "--deep", "--strict", app], 120)
    result = subprocess.run(["/usr/bin/codesign", "-dv", "--verbose=4", str(app)], capture_output=True, text=True, timeout=20)
    if expected_id == IDS["CCBaga-A.app"] and info.get("MingleSelfMaintained") is True:
        if result.returncode or "Signature=adhoc" not in result.stderr.splitlines():
            raise RuntimeError("Expected explicit self-maintained ad-hoc signature")
        manifest = json.loads((app / "Contents/Resources/integration-pair.json").read_text())
        if manifest.get("local_version") != info.get("CFBundleShortVersionString"):
            raise RuntimeError("Integrated build version receipt mismatch")
        return info  # Never label this application as maintainer-signed or notarized.
    if result.returncode or "TeamIdentifier=" + TEAM not in result.stderr.splitlines():
        raise RuntimeError("Official publisher mismatch")
    command(["/usr/sbin/spctl", "--assess", "--type", "execute", app], 90)
    return info


class Installation:
    """Durable rename transaction; test paths are injected only by the test suite."""
    def __init__(self, root=ROOT, apps=Path("/Applications"), data=None, state_dir=None, verifier=verify):
        self.root, self.apps = Path(root), Path(apps)
        self.data = Path(data) if data else Path.home() / "Library/Application Support"
        self.state_dir = Path(state_dir) if state_dir else Path.home() / ".local/state/chengjing/ccbaga-update"
        self.state_file = self.root / "CCBaga/current-version.json"
        self.journal = self.state_dir / "transaction.json"
        self.verifier = verifier

    def checkpoint(self, phase):
        pass

    def validate_journal(self, journal):
        stage = Path(journal["stage"])
        if stage.is_symlink() or stage.parent != self.apps or not re.fullmatch(r"\.chengjing-ccbaga-[a-z0-9_]+", stage.name):
            raise RuntimeError("Invalid recovery stage")
        if stage.exists() and stage.stat().st_uid != os.getuid():
            raise RuntimeError("Recovery stage owner changed")
        for name in journal["old_names"]:
            if name not in IDS:
                raise RuntimeError("Invalid recovery app")
        for source, destination in journal["data_moves"]:
            allowed = {self.data / "ai.local.ccbaga-tw-dev", self.data / "ai.local.requirement-french"}
            destination = Path(destination)
            if Path(source) not in allowed or not (destination in allowed or (destination.parent == self.data / "Chengjing/retained-user-data" and destination.name.startswith("ccbaga-official-"))):
                raise RuntimeError("Invalid profile recovery path")
        return stage

    def recover(self):
        if not self.journal.exists():
            return False
        journal = json.loads(self.journal.read_text())
        stage = self.validate_journal(journal)
        if journal["phase"] == "committed":
            for name in OFFICIAL:
                self.verifier(self.apps / name, IDS[name])
        else:
            # Stage/new absence proves this transaction moved that candidate into Applications.
            for name in OFFICIAL:
                target = self.apps / name
                if not (stage / "new" / name).exists() and target.exists():
                    with (target / "Contents/Info.plist").open("rb") as file:
                        if plistlib.load(file).get("CFBundleIdentifier") != IDS[name]:
                            raise RuntimeError("Recovery target changed; retained for inspection")
                    shutil.rmtree(target)
            for name in journal["old_names"]:
                old, target = stage / "old" / name, self.apps / name
                if old.exists():
                    if target.exists():
                        raise RuntimeError("Recovery app destination occupied")
                    old.rename(target)
            for source, destination in reversed(journal["data_moves"]):
                source, destination = Path(source), Path(destination)
                if destination.exists() and not source.exists():
                    destination.rename(source)
            atomic(self.state_file, journal["old_state"])
        if stage.exists():
            if journal["phase"] == "committed" and journal.get("downstream"):
                retained=self.root/"CCBaga/operations/rollback"/stage.name
                retained.parent.mkdir(parents=True,exist_ok=True)
                for app in (stage/"old").glob("*.app"):
                    app.rename(app.with_name(app.name+".saved"))
                shutil.move(str(stage),str(retained))
                atomic(retained/"committed-transaction.json",journal)
            else:
                shutil.rmtree(stage)
        self.journal.unlink()
        return True

    def install(self, candidates, release):
        old_state = json.loads(self.state_file.read_text())
        stage = Path(tempfile.mkdtemp(prefix=".chengjing-ccbaga-", dir=self.apps))
        os.chmod(stage, 0o700)
        atomic(stage / "owner.json", {"owner": "chengjing-ccbaga-update", "schema": 1})
        (stage / "new").mkdir()
        (stage / "old").mkdir()
        started = False
        try:
            for name in OFFICIAL:
                command(["/usr/bin/ditto", candidates / name, stage / "new" / name], 180)
                self.verifier(stage / "new" / name, IDS[name])
            old_names = []
            for name in IDS:
                app = self.apps / name
                if app.exists():
                    if app.is_symlink():
                        raise RuntimeError("Existing app is a redirect")
                    with (app / "Contents/Info.plist").open("rb") as file:
                        if plistlib.load(file).get("CFBundleIdentifier") != IDS[name]:
                            raise RuntimeError("Conflicting installed app identity")
                    old_names.append(name)
            data_moves = []
            source = self.data / "ai.local.ccbaga-tw-dev"
            target = self.data / "ai.local.requirement-french"
            if old_state["active_channel"] == "local-patched" and not release.get("_downstream"):
                if source.is_symlink() or target.is_symlink():
                    raise RuntimeError("Profile redirects require inspection")
                if target.exists():
                    retained = self.data / "Chengjing/retained-user-data" / ("ccbaga-official-" + stage.name.rsplit("-", 1)[1])
                    retained.parent.mkdir(parents=True, exist_ok=True)
                    data_moves.append([str(target), str(retained)])
                if source.exists():
                    data_moves.append([str(source), str(target)])
            if running(self.apps):
                raise RuntimeError("CCBaga became active; update deferred")
            journal = {"phase": "prepared", "stage": str(stage), "old_names": old_names, "old_state": old_state,
                       "data_moves": data_moves, "release": release["tag_name"], "downstream":release.get("_downstream")}
            atomic(self.journal, journal)
            started = True
            self.checkpoint("prepared")
            for source, target in data_moves:
                Path(source).rename(target)
            self.checkpoint("profiles-moved")
            for name in old_names:
                (self.apps / name).rename(stage / "old" / name)
            self.checkpoint("old-apps-moved")
            for name in OFFICIAL:
                (stage / "new" / name).rename(self.apps / name)
            self.checkpoint("new-apps-moved")
            for name in OFFICIAL:
                self.verifier(self.apps / name, IDS[name])
            updated = dict(old_state)
            updated.update(updated_at=now(), active_channel="official", base_release=release["tag_name"],
                           local_version=release["tag_name"].lstrip("v"), apps=[str(self.apps / n) for n in OFFICIAL],
                           app_data="~/Library/Application Support/ai.local.requirement-french", installed_source_ref=release["tag_name"],
                           automatic_background_update=True, update_execution="launchd one-shot, no resident worker")
            if release.get("_downstream"):
                pair=release["_downstream"]
                updated.update(active_channel="self-maintained-integrated",local_version=pair["local_version"],
                    source="CCBaga/active-integration",source_absolute=pair["source"],source_commit=pair["source_commit"],
                    source_branch="downstream/mingle-browser-environments",installed_source_ref=pair["source_commit"],
                    release_source_sha=pair["release_source_sha"],installed_asset_fingerprint=pair["asset_fingerprint"],
                    pair_receipt=pair,signature_policy="A self-maintained ad-hoc, not notarized; B unmodified official engine",
                    standalone_environments="~/Library/Application Support/ai.local.mingle.ccbaga.environments/browser/environments",
                    update_policy="Only real release/manual events run the downstream pre-update hook; never install plain official A over the overlay",
                    installation_policy="One A+B group; retain original apps by journal/rollback, no profile moves or copies")
            if release.get("_restore_state"):
                updated=dict(release["_restore_state"])
            atomic(self.state_file, updated)
            self.checkpoint("state-written")
            journal["phase"] = "committed"
            atomic(self.journal, journal)
            self.checkpoint("committed")
            # These are generated host connection hints, not account/session records.
            for name in ["start.url", "start.url.tmp", "launch.args"]:
                (self.data / "ai.local.requirement-french/browser" / name).unlink(missing_ok=True)
            if release.get("_downstream"):
                self.recover()  # Commit recovery archives the exact previous pair instead of deleting it.
            else:
                shutil.rmtree(stage)
                self.journal.unlink()
            for name in OFFICIAL:
                subprocess.run([REGISTER, "-f", str(self.apps / name)], capture_output=True, timeout=20)
            return updated
        except BaseException:
            if started:
                self.recover()
            elif stage.exists():
                shutil.rmtree(stage)
            raise


def candidate_sources(tag, destination):
    files = ["src-tauri/src/regions.rs", "src-tauri/src/browser/config.rs", "src-tauri/src/browser/mod.rs",
             "src-tauri/src/browser/server.rs", "src-tauri/native/Browser.m", "src/browser-model.ts", "src/browser-verification.ts"]
    from urllib.parse import quote
    for name in files:
        value = github("contents/" + name + "?ref=" + quote(tag, safe=""))
        if value.get("encoding") != "base64" or value.get("size", 0) > 1048576:
            raise RuntimeError("Unexpected compatibility source response")
        path = destination / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(base64.b64decode(value["content"], validate=False))


def sandboxed(args, folder, timeout=60):
    """Candidate code sees only toolchains/staged sources; no network, credentials, app writes or UI launch."""
    def literal(value):
        return json.dumps(str(Path(value).resolve()), ensure_ascii=False)
    developer = command(["/usr/bin/xcode-select", "-p"]).strip()
    bun = (ROOT / "CCBaga/development/tools/node_modules/.bin/bun").resolve()
    rustup = (Path.home() / ".cargo/bin/rustc").resolve()
    read_paths = ["/System", "/usr", "/bin", "/sbin", "/Library/Developer", "/Library/Apple", "/Library/Preferences", "/private/etc", "/private/var/db/timezone", developer,
                  str(Path.home() / ".rustup"), str(ROOT / "CCBaga/development/tools"), str(folder)]
    executable_paths = [bun, rustup, "/usr/bin/xcrun", "/usr/bin/cc", "/usr/bin/clang", "/usr/bin/ld", folder / "native-regression"]
    profile = '(version 1)(allow default)(deny network*)(deny file-read* (subpath "/Users")(subpath "/Volumes")(subpath "/private/var/folders")(subpath "/private/tmp")(subpath "/System/Volumes/Data/Users")(subpath "/System/Volumes/Data/Volumes"))(deny file-write*)(deny process-exec)(allow file-read-metadata)(deny appleevent-send)(deny mach-lookup (global-name "com.apple.coreservices.launchservicesd")(global-name "com.apple.lsd.mapdb"))' 
    profile += "(allow file-read* " + " ".join("(subpath " + literal(p) + ")" for p in read_paths) + ")(allow file-read* (subpath \"/dev\"))"
    profile += "(allow file-write* (subpath " + literal(folder) + ")(literal \"/dev/null\"))"
    profile += "(allow file-read* (literal " + literal(rustup) + "))"
    profile += "(allow process-exec " + " ".join("(literal " + literal(p) + ")" for p in executable_paths)
    profile += "(subpath " + literal(Path.home() / ".rustup/toolchains") + ")(subpath " + literal(Path(developer) / "Toolchains") + ")(subpath " + literal(Path(developer) / "usr/bin") + "))"
    profile += '(allow mach-lookup (global-name "com.apple.system.logger")(global-name "com.apple.cfprefsd.daemon")(global-name "com.apple.cfprefsd.agent"))'
    env = dict(os.environ)
    env.update(TMPDIR=str(folder), BUN_RUNTIME_TRANSPILER_CACHE_PATH=str(folder / "bun-cache"), DEVELOPER_DIR=developer, SDKROOT=str(Path(developer) / "Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk"))
    result = subprocess.run(["/usr/bin/sandbox-exec", "-p", profile, *map(str, args)], env=env, cwd=folder, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError("Isolated regression failed (" + str(result.returncode) + "): " + result.stderr.strip()[-700:])
    return result.stdout


def regression(folder):
    """Run the reviewed golden cases against candidate code, without launching apps or changing identity."""
    repo = ROOT / "CCBaga/development/CCBaga"
    git = next((p for p in [Path("/opt/homebrew/bin/git"), Path("/Applications/Xcode.app/Contents/Developer/usr/bin/git")] if p.is_file()), None)
    if git is None:
        raise RuntimeError("Existing Git unavailable; do not invoke a developer-tools installer")
    fixture = command([git, "-C", repo, "show", PIN + ":src/browser-verification.test.ts"])
    (folder / "src/browser-verification.test.ts").write_text(fixture)
    bun = ROOT / "CCBaga/development/tools/node_modules/.bin/bun"
    sandboxed([bun, "test", folder / "src/browser-verification.test.ts"], folder, 60)
    regions = (folder / "src-tauri/src/regions.rs").read_text()
    regions = re.sub(r"(?ms)^#\[cfg\(test\)\]\s*\nmod tests \{.*?^\}\s*\n", "", regions)
    regions = regions.replace("use serde::Serialize;", "").replace("#[derive(Serialize)]", "").replace("#[tauri::command]", "")
    config = (folder / "src-tauri/src/browser/config.rs").read_text()
    methods = []
    for name in ["region", "language", "timezone"]:
        match = re.search(r"(?ms)^    pub fn " + name + r"\(.*?^    \}", config)
        if not match:
            raise RuntimeError("Compatibility method changed: " + name)
        methods.append(match[0])
    native = (folder / "src-tauri/native/Browser.m").read_text()
    server = (folder / "src-tauri/src/browser/server.rs").read_text()
    module = (folder / "src-tauri/src/browser/mod.rs").read_text()
    if not ('@"locale_country"' in native and '.countryCode' in native and 'locale_country' in config and
            'current.update_machine(machine)' in server and 'self.region = self.config.region(&machine)' in module):
        raise RuntimeError("Effective native region/context refresh contract not present")
    harness = "mod regions {\n" + regions + "\n}\n" + r'''
struct Machine {locale_country: String, timezone: String, languages: Vec<String>}
struct Config {follow_machine: bool, language: String, timezone: String}
impl Config {
''' + "\n".join(methods) + r'''
}
fn main() {
 for l in ["zh_TW", "zh-Hans-TW", "zh_CN@rg=twzzzz", "zh-CN-u-rg-twzzzz"] {
  assert_eq!(regions::locale_country(l).as_deref(), Some("TW"));
 }
 for l in ["", "C", "zh-Hans", "zh_CN@rg=invalid", "C@rg=twzzzz", "zh--CN@rg=twzzzz"] {
  assert_eq!(regions::locale_country(l), None);
 }
 assert_eq!(regions::locale_country("zh-CN-x-u-rg-twzzzz").as_deref(),Some("CN"));
 assert!(regions::matching_country("CN","Asia/Taipei").is_none());
 assert!(regions::matching_country("TW","Asia/Shanghai").is_none());
 let m=Machine {locale_country:"TW".into(), timezone:"Asia/Taipei".into(), languages:vec!["zh-Hans-TW".into()]};
 let c=Config {follow_machine:true, language:"en-US".into(),timezone:"America/New_York".into()};
 assert_eq!(c.region(&m).unwrap().country,"TW");
 assert_eq!(c.language(&m),"zh-Hans-TW");
 assert_eq!(c.timezone(&m),"Asia/Taipei");
 let bad=Machine{locale_country:"CN".into(),..m}; assert!(c.region(&bad).is_none());
}
'''
    path = folder / "native-regression.rs"
    path.write_text(harness)
    binary = folder / "native-regression"
    linker = Path(command(["/usr/bin/xcode-select", "-p"]).strip()) / "Toolchains/XcodeDefault.xctoolchain/usr/bin/clang"
    if not linker.is_file():
        raise RuntimeError("Existing Xcode compiler unavailable; do not invoke a tool installer")
    sandboxed([Path.home() / ".cargo/bin/rustc", "--edition=2021", "-C", "linker=" + str(linker), "--crate-name", "chengjing_gate", path, "-o", binary], folder, 60)
    sandboxed([binary], folder, 15)


def download_environment():
    """Use the configured macOS HTTPS proxy for the asset only; do not mutate system settings."""
    env = os.environ.copy()
    if any(env.get(name) for name in ("HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy")):
        return env
    proxy = urllib.request.getproxies().get("https")
    if proxy:
        parsed = urllib.parse.urlsplit(proxy)
        if parsed.scheme not in ("http", "https") or not parsed.hostname or parsed.username or parsed.password:
            raise RuntimeError("Unsupported system HTTPS proxy; existing installation retained")
        env["HTTPS_PROXY"] = proxy
    return env


def download_asset(release, folder):
    expected_name = "CCBaga-" + release["tag_name"].lstrip("v") + "-macos-arm64.dmg"
    matches = [a for a in release.get("assets", []) if a.get("name") == expected_name]
    if len(matches) != 1:
        raise RuntimeError("Official Apple Silicon A+B installer not found")
    asset = matches[0]
    digest = asset.get("digest", "")
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest) or asset.get("size", 0) > 1500000000:
        raise RuntimeError("Release asset lacks an acceptable publisher digest")
    url = asset.get("browser_download_url", "")
    if not url.startswith("https://github.com/liseami/CCBaga/releases/download/"):
        raise RuntimeError("Unexpected release asset origin")
    command([GH, "release", "download", release["tag_name"], "--repo", "liseami/CCBaga", "--pattern", expected_name, "--dir", folder], 900, env=download_environment())
    path = folder / expected_name
    if path.stat().st_size != asset["size"]:
        raise RuntimeError("Installer size mismatch")
    with path.open("rb") as file:
        actual = hashlib.file_digest(file, "sha256").hexdigest()
    if actual != digest.split(":", 1)[1]:
        raise RuntimeError("Installer SHA256 mismatch")
    return path


def attach_image(image, mount):
    # macOS 27 replaces the legacy attach interface; use local temporary mount points.
    probe = subprocess.run(["/usr/sbin/diskutil", "image", "attach", "--help"], capture_output=True, text=True, timeout=15)
    if probe.returncode == 0 and "--mountPoint" in probe.stdout:
        output = command(["/usr/sbin/diskutil", "image", "attach", "--readOnly", "--nobrowse", "--mountPoint", mount, image], 90)
    else:
        mount.mkdir(exist_ok=True)
        output = command(["/usr/bin/hdiutil", "attach", "-readonly", "-nobrowse", "-noautoopen", "-mountpoint", mount, image], 90)
    match = re.search(r"^(/dev/disk[0-9]+)\s", output, re.M)
    if not match:
        raise RuntimeError("Attached image did not report a whole device")
    return match[1]


def detach_image(device):
    if not re.fullmatch(r"/dev/disk[0-9]+", device):
        raise RuntimeError("Invalid image device")
    command(["/usr/sbin/diskutil", "eject", device], 60)


def reap_update_mounts(manager):
    # Only our own image files/stages; never eject unrelated images or user disks.
    info = subprocess.run(["/usr/bin/hdiutil", "info", "-plist"], capture_output=True, timeout=20)
    if info.returncode:
        raise RuntimeError("Cannot inspect prior installer mounts")
    for image in plistlib.loads(info.stdout).get("images", []):
        path = Path(image.get("image-path", "/"))
        if path.parent.parent != manager.root / "CCBaga" or not path.parent.name.startswith(".update-"):
            continue
        entities = image.get("system-entities", [])
        whole = next((e.get("dev-entry") for e in entities if re.fullmatch(r"/dev/disk[0-9]+", e.get("dev-entry", ""))), None)
        if whole:
            detach_image(whole)
    for temporary in Path(tempfile.gettempdir()).glob("chengjing-ccbaga-dmg-*"):
        if temporary.is_dir() and not temporary.is_symlink() and temporary.stat().st_uid == os.getuid() and not os.path.ismount(temporary / "volume"):
            shutil.rmtree(temporary)


def apply_release(manager, release, folder):
    dmg = download_asset(release, folder)
    with tempfile.TemporaryDirectory(prefix="chengjing-ccbaga-dmg-") as temporary:
        mount = Path(temporary) / "volume"
        device = attach_image(dmg, mount)
        try:
            for name in OFFICIAL:
                info = manager.verifier(mount / name, IDS[name])
                if name == OFFICIAL[0] and version(info["CFBundleShortVersionString"]) != version(release["tag_name"]):
                    raise RuntimeError("Installer version does not match release")
            spec = importlib.util.spec_from_file_location("ccbaga_resource_check", ROOT / "CCBaga/development/CCBaga/scripts/browser_ui.py")
            helper = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(helper)
            helper.verify_browser_ui(mount / OFFICIAL[0] / "Contents/Resources/browser-ui")
            if running(manager.apps):
                return "deferred-app-running"
            manager.install(mount, release)
            return "updated"
        finally:
            detach_image(device)


def restore_retained(manager, directory):
    directory=Path(directory).resolve()
    allowed=(manager.root/"CCBaga/operations/rollback").resolve()
    if not directory.is_relative_to(allowed) or directory.is_symlink() or directory.stat().st_uid!=os.getuid():
        raise RuntimeError("Invalid rollback receipt directory")
    receipt=json.loads((directory/"committed-transaction.json").read_text())
    if running(manager.apps):return {"status":"deferred-app-running","installed":False}
    current=json.loads(manager.state_file.read_text())
    base=manager.root/"CCBaga/_trash";base.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="restore-",dir=base) as temporary:
        candidates=Path(temporary)
        for name in OFFICIAL:
            command(["/usr/bin/ditto",directory/"old"/(name+".saved"),candidates/name],180)
            manager.verifier(candidates/name,IDS[name])
        release={"tag_name":receipt["old_state"]["base_release"],"_restore_state":receipt["old_state"]}
        if current.get("pair_receipt"):release["_downstream"]=current["pair_receipt"]
        manager.install(candidates,release)
    return {"status":"restored-retained-pair","installed":True,"restored_version":receipt["old_state"]["local_version"],"profile_copy_or_move":False}


def execute(manager, check_only=False, integrate_current=False, stage_only=False, rollback=None):
    manager.state_dir.mkdir(parents=True, exist_ok=True)
    with (manager.state_dir / "run.lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return {"status": "another-run-active"}
        recovered = manager.recover()
        if rollback:return restore_retained(manager,rollback)
        # Source/overlay checks are event-driven; no periodic overlay polling.
        # Recovery above is the only pre-version-check mutation.
        state = json.loads(manager.state_file.read_text())
        release = github("releases/latest")
        result = {"checked_at": now(), "status": "up-to-date", "active_version": state["local_version"],
                  "latest_release": release["tag_name"], "recovery_performed": recovered, "installed": False}
        current_asset=state.get("installed_asset_fingerprint")
        config_path=manager.root/"CCBaga/downstream-update.json"
        republished=False
        if current_asset and config_path.exists() and version(release["tag_name"]) == version(state["base_release"]):
            from integrated_update import asset_key
            republished=asset_key(release)!=current_asset
        if not integrate_current and not republished and version(release["tag_name"]) <= version(state["base_release"]):
            return result
        if check_only:
            return dict(result, status="new-release-available")
        if config_path.exists() or state.get("active_channel")=="self-maintained-integrated":
            if not config_path.exists():raise RuntimeError("Integrated configuration missing; refusing plain official overwrite")
            from integrated_update import apply, asset_key
            previous=manager.root/"CCBaga/operations/upstream-update/status.json"
            if previous.exists() and not integrate_current:
                event=json.loads(previous.read_text())
                if event.get("status")=="blocked-integrated-update" and event.get("asset_fingerprint")==asset_key(release):
                    return dict(result,status="blocked-update-cached",event_id=event.get("event_id"))
            if running(manager.apps) and not stage_only:
                return dict(result,status="deferred-app-running")
            return apply(manager,release,globals(),stage_only=stage_only)
        reap_update_mounts(manager)
        # Reap our own interrupted download/staging work; never delete recoverable old apps.
        for stage in manager.apps.glob(".chengjing-ccbaga-*"):
            if stage.is_symlink() or stage.stat().st_uid != os.getuid():
                continue
            marker = stage / "owner.json"
            if marker.is_file() and json.loads(marker.read_text()).get("owner") == "chengjing-ccbaga-update" and not any((stage / "old").glob("*.app")):
                shutil.rmtree(stage)
        for stale in (manager.root / "CCBaga").glob(".update-*"):
            if stale.is_symlink() or stale.stat().st_uid != os.getuid():
                continue
            mount = stale / "mount"
            if os.path.ismount(mount):
                command(["/usr/bin/hdiutil", "detach", mount], 60)
            shutil.rmtree(stale)
        fingerprint = release["tag_name"] + ":" + ",".join(a.get("digest", "") or "" for a in release.get("assets", []) if a.get("name", "").endswith("macos-arm64.dmg"))
        cache = manager.state_dir / "incompatible-release.json"
        if cache.exists() and json.loads(cache.read_text()).get("fingerprint") == fingerprint:
            return dict(result, status="kept-current-compatibility-gate-cached")
        if running(manager.apps):
            return dict(result, status="deferred-app-running")
        with tempfile.TemporaryDirectory(prefix=".update-", dir=manager.root / "CCBaga") as temp:
            folder = Path(temp)
            candidate_sources(release["tag_name"], folder)
            try:
                regression(folder)
            except (RuntimeError, OSError, ValueError):
                atomic(cache, {"fingerprint": fingerprint, "checked_at": now(), "result": "incompatible"})
                return dict(result, status="kept-current-compatibility-gate", installed=False)
            result["compatibility_gate"] = "passed: golden frontend negatives + native region/language cases + refresh contract"
            result["status"] = apply_release(manager, release, folder)
            result["installed"] = result["status"] == "updated"
            return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("--rollback", type=Path, help="Restore a retained original pair with the same lock/journal; no user profile moves")
    parser.add_argument("--integrate-current", action="store_true", help="Explicit user update: rebuild the integrated current release")
    parser.add_argument("--stage-only", action="store_true", help="Run the same event pipeline without installation")
    args = parser.parse_args()
    manager = Installation()
    def deadline(signum, frame):
        raise TimeoutError("One-shot update deadline reached")
    signal.signal(signal.SIGALRM, deadline)
    signal.alarm(1800)
    try:
        result = execute(manager, args.check_only, args.integrate_current, args.stage_only, args.rollback)
    except Exception as error:
        current = json.loads(manager.state_file.read_text()) if manager.state_file.exists() else {}
        result = {"checked_at": now(), "status": "error-current-install-retained", "active_version": current.get("local_version"),
                  "active_channel": current.get("active_channel"), "error": str(error)[:1000], "recovery_pending": manager.journal.exists()}
    if result.get("status") not in ("up-to-date","another-run-active","blocked-update-cached"):
        atomic(ROOT / "CCBaga/operations/latest-auto-update.json", result)
    signal.alarm(0)
    if not args.quiet and result.get("status") != "up-to-date":
        print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
