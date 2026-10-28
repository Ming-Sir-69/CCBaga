#!/usr/bin/env python3
# [INPUT]: crates.io glib 0.18.5 的逐文件摘要、上游迭代器修复与本地 Cargo 覆盖。
# [OUTPUT]: 验证回补仅含上游两行；--native 在优化构建中执行上游真实 GLib 迭代器回归。
# [POS]: GTK3 兼容安全回补的来源与运行门槛；不忽略公告、不伪造依赖版本。
# [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parent.parent
VENDOR = ROOT / 'vendor/glib'
ARCHIVE = '233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5'
FIX = 'https://github.com/gtk-rs/gtk-rs-core/commit/b5a4071e439bef2b5eea76c3aa25e5ae84839e34'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--native', action='store_true')
    args = parser.parse_args()
    source = json.loads((VENDOR / 'UPSTREAM.json').read_text())
    assert source['archive_sha256'] == ARCHIVE and source['upstream_fix'] == FIX
    assert source['package'] == 'glib' and source['version'] == '0.18.5'
    for name, expected in source['files'].items():
        path = VENDOR / name
        assert path.is_file() and not path.is_symlink(), name
        data = path.read_bytes()
        if name == 'src/variant_iter.rs':
            assert hashlib.sha256(data).hexdigest() == source['patched_sha256']
            assert data.count(b'let mut p: *mut libc::c_char = std::ptr::null_mut();') == 1
            assert data.count(b'                &mut p,') == 1
            data = data.replace(b'let mut p: *mut libc::c_char', b'let p: *mut libc::c_char')
            data = data.replace(b'                &mut p,', b'                &p,')
        assert hashlib.sha256(data).hexdigest() == expected, name
    for path in VENDOR.rglob('*'):
        if path.is_file() and path.name not in {'CLAUDE.md', 'UPSTREAM.json', 'Cargo.lock'}:
            assert path.relative_to(VENDOR).as_posix() in source['files'], path
    manifest = tomllib.loads((ROOT / 'src-tauri/Cargo.toml').read_text())
    assert manifest['patch']['crates-io']['glib']['path'] == '../vendor/glib'
    lock = tomllib.loads((ROOT / 'src-tauri/Cargo.lock').read_text())
    libraries = [p for p in lock['package'] if p['name'] == 'glib' and p['version'] == '0.18.5']
    assert len(libraries) == 1 and 'source' not in libraries[0]
    print('GLib 0.18.5: all upstream bytes verified; only the official two-line safety fix differs.')
    if args.native:
        subprocess.run([
            'cargo', 'test', '--manifest-path', str(VENDOR / 'Cargo.toml'),
            '--target-dir', str(ROOT / 'src-tauri/target/glib-backport'),
            '--release', '--lib', 'variant_iter::tests',
        ], cwd=ROOT, check=True)


if __name__ == '__main__':
    main()
