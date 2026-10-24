/** Read-only browser metadata; this dialog never copies or opens a discovered profile. */
import {useEffect, useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Button} from 'cromma-ui';
import {AppDialog} from './dialog';
import {useI18n} from './i18n';

export type DiscoveredBrowser = {
    id: string; name: string; bundle_id: string; kind: 'chromium' | 'safari' | 'other';
    installed: boolean; is_default: boolean;
    installations: {bundle_id: string; name: string; path: string; version: string | null}[];
    data_root: string | null; data_state: string; profile_state: string;
    profiles: {id: string; name: string}[]; import_capability: 'unavailable';
};
export type BrowserDiscovery = {supported: boolean; default_known: boolean; items: DiscoveredBrowser[]};

export function BrowserAddDialog({open, disabled = false, onOpenChange, onCreate}: {
    open: boolean; disabled?: boolean; onOpenChange: (open: boolean) => void; onCreate: (temporary: boolean) => void;
}) {
    const {t} = useI18n();
    const [result, setResult] = useState<BrowserDiscovery | null>(null);
    const [error, setError] = useState('');
    useEffect(() => {
        if (!open) return;
        let live = true;
        setResult(null); setError('');
        void invoke<BrowserDiscovery>('browser_discover', {profilesEnabled: true}).then(
            next => { if (live) setResult(next); },
            reason => { if (live) setError(String(reason)); },
        );
        return () => { live = false; };
    }, [open]);
    const found = result?.items.filter(item => item.installed || item.is_default || item.data_state !== 'absent') || [];
    const present = found.filter(item => item.kind !== 'other' || item.is_default);
    const handlers = found.filter(item => item.kind === 'other' && !item.is_default);
    const absent = result?.items.filter(item => !item.installed && !item.is_default && item.data_state === 'absent') || [];
    function profileState(item: DiscoveredBrowser) {
        if (item.kind === 'safari') return t("Safari 导入待支持");
        switch (item.profile_state) {
            case 'ready': return t("发现 {v0} 个资料", {v0: item.profiles.length});
            case 'empty': return t("未发现资料");
            case 'restricted': return t("资料访问受限");
            case 'disabled': return t("未读取资料名称");
            case 'unsupported': return t("资料发现待支持");
            default: return t("资料状态未知");
        }
    }
    return <AppDialog open={open} onOpenChange={onOpenChange} title={t("添加环境")}>
        <div className="browser-add-body">
            <div className="browser-actions">
                <Button variant="ink" disabled={disabled} onClick={() => { onOpenChange(false); onCreate(false); }}>{t("新建长期环境")}</Button>
                <Button variant="secondary" disabled={disabled} onClick={() => { onOpenChange(false); onCreate(true); }}>{t("新建一次性环境")}</Button>
            </div>
            <div className="browser-discovery-title"><h3>{t("本机浏览器")}</h3><span>{t("仅发现，导入尚未开放")}</span></div>
            {error && <p role="alert" className="system-feedback error">{t("浏览器发现失败")}: {t(error)}</p>}
            {!result && !error && <p role="status">{t("正在发现浏览器")}</p>}
            {result && !result.supported && <p role="status">{t("此平台暂不支持浏览器发现")}</p>}
            {result?.supported && !result.default_known && <p className="system-hint">{t("默认浏览器未知")}</p>}
            {result?.supported && present.length === 0 && <p>{t("未发现本机浏览器")}</p>}
            <ul className="browser-discovery-list">
                {present.map(item => <li key={item.id}>
                    <div className="browser-discovery-heading"><strong>{item.name}</strong>{item.kind === 'other' && <span>{t("网页处理程序")}</span>}{item.is_default && <span>{t("默认浏览器")}</span>}
                        <Button size="small" variant="secondary" disabled title={t("仅发现，导入尚未开放")}>{t("导入")}</Button>
                    </div>
                    <p className="system-hint">{item.installed ? t("已安装") : item.data_state === 'present' ? t("未安装 · 有历史资料") : t("未安装")} · {profileState(item)}</p>
                    {!!item.profiles.length && <ul className="browser-profile-names">{item.profiles.map(profile => <li key={profile.id}>{profile.name}</li>)}</ul>}
                    <details><summary>{t("详情")}</summary><dl>
                        <dt>{t("浏览器标识")}</dt><dd>{item.bundle_id}</dd>
                        {item.installations.map(app => <div key={app.path}><dt>{app.version || t("版本未知")}</dt><dd>{app.path}</dd></div>)}
                        {item.data_root && <><dt>{t("资料位置")}</dt><dd>{item.data_root}</dd></>}
                    </dl></details>
                </li>)}
            </ul>
            {!!handlers.length && <details className="browser-discovery-absent"><summary>{t("其他网页处理程序")}</summary><ul>{handlers.map(item => <li key={item.id}>
                <strong>{item.name}</strong> · {t("资料发现待支持")}
                <details><summary>{t("详情")}</summary><p>{item.bundle_id}</p>{item.installations.map(app => <p key={app.path}>{app.version || t("版本未知")} · {app.path}</p>)}</details>
            </li>)}</ul></details>}
            {!!absent.length && <details className="browser-discovery-absent"><summary>{t("未安装浏览器")}</summary><ul>{absent.map(item => <li key={item.id}>{item.name} · {t("未安装")}</li>)}</ul></details>}
        </div>
    </AppDialog>;
}
