/**
 * [INPUT]: 独立环境命令、受限配置、只读机器时区目录、原生环境卡数据与详情，以及共享 YAML 文案。
 * [OUTPUT]: 日常 Brave 固定最左且无关闭/清理，原生环境保留身份联动；外部引用仅解除登记；原生环境、独立环境按名称排列的完整封闭卡片；显式原生/UUID 选择目标，选中外框连接唯一常显详情区；独立指纹环境内嵌配置并读回，未保存草稿时禁止打开，删除在详情末尾；Brave 沿用原设置。
 * [POS]: 浏览器目录与选择边界；不切换 Claude 身份，不向独立环境展示原生缓存或实测报告，不接触资料路径。
 * [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
 */
import {useEffect, useLayoutEffect, useRef, useState, type ReactNode} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Button, Checkbox, Input, Panel, PanelHeader, Select} from 'cromma-ui';
import {AppDialog} from './dialog';
import {BrowserAddDialog} from './browser-discovery';
import {useI18n} from './i18n';
import {NATIVE_TARGET, environmentDirty, environmentDraft, resolveBrowserTarget, targetKey, withoutDraft, type BrowserConfig, type EnvironmentDrafts, type EnvironmentSnapshot, type EnvironmentSummary} from './browser-model';

type Creator = {name: string; temporary: boolean};
/** The native card stays owned by BrowserPanel; running=null means its state has not been read back. */
export type NativeCard = {running: boolean | null; linked: boolean; summary: string | null; footer: ReactNode; detail: ReactNode};
const languages = ['en-US', 'en-CA', 'en-GB', 'en-SG', 'en-AU', 'en', 'zh-Hans', 'zh-Hant', 'fr', 'de', 'es', 'it', 'ja', 'ko'];
const STORAGE = 'browser-environment-selected-v2';

export function BrowserEnvironments({disabled = false, timezones = [], native, onBusy, onRefresh}: {
    disabled?: boolean;
    timezones?: string[];
    native: NativeCard;
    onBusy?: (busy: boolean) => void;
    onRefresh?: () => Promise<void>;
}) {
    const {t} = useI18n();
    const [snapshot, setSnapshot] = useState<EnvironmentSnapshot | null>(null);
    const [busy, setBusy] = useState(false), [error, setError] = useState('');
    const [pending, setPending] = useState<{command:string; id?:string}|null>(null);
    const [creator, setCreator] = useState<Creator | null>(null);
    const [renaming, setRenaming] = useState<EnvironmentSummary | null>(null);
    const [renameValue, setRenameValue] = useState('');
    const [removing, setRemoving] = useState<EnvironmentSummary | null>(null);
    const [accepted, setAccepted] = useState(false);
    const [adding, setAdding] = useState(false);
    // Unsaved edits are kept per UUID, so one environment's draft can never be saved into another.
    const [drafts, setDrafts] = useState<EnvironmentDrafts>({});
    const gate = useRef(false), reading = useRef(false), live = useRef(true), revision = useRef(0);
    const [stored, setStored] = useState<string | null>(() => {
        try { return localStorage.getItem(STORAGE); } catch { return null; }
    });
    const target = resolveBrowserTarget(snapshot?.items ?? null, stored);
    const key = targetKey(target);
    const active = target?.kind === 'environment' ? snapshot?.items.find(item => item.id === target.id) ?? null : null;
    const cardGroup = useRef<HTMLDivElement>(null);
    const [outline, setOutline] = useState('');
    useLayoutEffect(() => {
        const group = cardGroup.current;
        if (!group) return;
        const rail = group.querySelector<HTMLElement>('.environment-card-selector');
        const report = group.querySelector<HTMLElement>('.environment-detail');
        if (!rail || !report) return;
        function measure() {
            const card = group!.querySelector<HTMLElement>('.environment-card-frame.selected');
            const box = group!.getBoundingClientRect(), panel = report!.getBoundingClientRect();
            const w = box.width - 1, h = box.height - 1, t = panel.top - box.top + 1, r = 16;
            const c = card?.getBoundingClientRect();
            if (!c || c.right <= box.left || c.left >= box.right) {
                setOutline(`M ${r+1} ${t} H ${w-r} Q ${w} ${t} ${w} ${t+r} V ${h-r} Q ${w} ${h} ${w-r} ${h} H ${r+1} Q 1 ${h} 1 ${h-r} V ${t+r} Q 1 ${t} ${r+1} ${t} Z`);
                return;
            }
            const x = Math.max(1, c.left-box.left), right = Math.min(w,c.right-box.left), y = c.top-box.top+1;
            const leftCorner = Math.min(r,Math.max(0,(x-1)/2)), rightCorner = Math.min(r,Math.max(0,(w-right)/2));
            // Card radius 16 + frame padding 8: the tab corner stays concentric with the card inside it.
            const cap = Math.min(24,(right-x)/2);
            setOutline(`M 1 ${t+leftCorner} Q 1 ${t} ${1+leftCorner} ${t} H ${x-leftCorner} Q ${x} ${t} ${x} ${t-leftCorner} V ${y+cap} Q ${x} ${y} ${x+cap} ${y} H ${right-cap} Q ${right} ${y} ${right} ${y+cap} V ${t-rightCorner} Q ${right} ${t} ${right+rightCorner} ${t} H ${w-rightCorner} Q ${w} ${t} ${w} ${t+rightCorner} V ${h-r} Q ${w} ${h} ${w-r} ${h} H ${r+1} Q 1 ${h} 1 ${h-r} Z`);
        }
        measure();
        const resize = new ResizeObserver(measure);
        resize.observe(group); resize.observe(rail); resize.observe(report);
        rail.addEventListener('scroll', measure, {passive:true});
        return () => { resize.disconnect(); rail.removeEventListener('scroll', measure); };
    }, [key, snapshot?.items.length]);
    const externalBusy = useRef(disabled);
    externalBusy.current = disabled;
    // Persist the resolved target, including the safe native fallback after a deletion.
    useEffect(() => {
        if (!key || key === stored) return;
        setStored(key);
        try { localStorage.setItem(STORAGE, key); } catch { /* Optional UI preference. */ }
    }, [key, stored]);
    // Card clicks select in the capture phase, before a card button starts its own locked action.
    function select(next: string) {
        if (gate.current || externalBusy.current) return;
        setStored(next); setError('');
        try { localStorage.setItem(STORAGE, next); } catch { /* Optional UI preference. */ }
    }

    useEffect(() => { onBusy?.(busy); return () => onBusy?.(false); }, [busy, onBusy]);
    async function read() {
        if (gate.current || reading.current || externalBusy.current) return;
        reading.current = true;
        const began = revision.current;
        try {
            const next = await invoke<EnvironmentSnapshot>('browser_environments_read');
            if (live.current && began === revision.current) setSnapshot(next);
        } catch (e) {
            if (live.current && began === revision.current) { setError(String(e)); setSnapshot(null); }
        } finally { reading.current = false; }
    }
    useEffect(() => {
        live.current = true;
        void read();
        const timer = window.setInterval(() => void read(), 2000);
        const focus = () => void read();
        window.addEventListener('focus', focus);
        return () => { live.current = false; window.clearInterval(timer); window.removeEventListener('focus', focus); };
    }, []);

    async function act(command: string, args: Record<string, unknown> = {}) {
        if (gate.current || externalBusy.current) return false;
        gate.current = true; revision.current += 1; setBusy(true); setPending({command, id: typeof args.id === 'string' ? args.id : undefined}); setError('');
        try {
            const next = await invoke<EnvironmentSnapshot>(command, args);
            if (live.current) setSnapshot(next);
            return next;
        } catch (e) {
            if (live.current) setError(String(e));
            try {
                const next = await invoke<EnvironmentSnapshot>('browser_environments_read');
                if (live.current) setSnapshot(next);
            } catch { if (live.current) setSnapshot(null); }
            return false;
        } finally { gate.current = false; if (live.current) { setBusy(false); setPending(null); } }
    }

    function status(item: EnvironmentSummary) {
        if (pending?.id === item.id && pending.command === 'browser_environment_quit') return t("正在关闭");
        if (pending?.id === item.id && pending.command === 'browser_environment_launch') return t("正在打开");
        switch (item.runtime_status) {
            case 'open': return t("已打开");
            case 'background': return t("后台运行");
            case 'residual': return t("后台未退出");
            case 'starting': return t("正在打开");
            case 'closing': return t("正在关闭");
            case 'checking': return t("正在确认");
            case 'unknown': return t("状态未知");
            case 'stopped': return t(item.cleanup_pending ? "等待清理" : "已关闭");
            default: return t(item.running ? "运行中" : item.cleanup_pending ? "等待清理" : "已关闭");
        }
    }
    const nativeStatus = t(native.running === null ? "状态未知" : native.running ? "运行中" : "已关闭");
    const background = (item:EnvironmentSummary) => item.runtime_status === 'background' || item.runtime_status === 'residual';
    const locked = busy || disabled;
    const removalCurrent = removing ? snapshot?.items.find(item => item.id === removing.id) : null;
    const renameCurrent = renaming ? snapshot?.items.find(item => item.id === renaming.id) : null;
    const renameValid = !!renameValue.trim() && Array.from(renameValue.trim()).length <= 80 && !/[\u0000-\u001f\u007f-\u009f]/.test(renameValue.trim());
    const creatorValid = !!creator?.name.trim() && Array.from(creator.name.trim()).length <= 80;
    // Brave keeps its own settings: its stored placeholder config is never shown or saved as a fingerprint.
    const draft = active ? environmentDraft(active, drafts) : null;
    const dirty = !!active && environmentDirty(active, drafts);
    const draftValid = !!draft && Number.isInteger(draft.seed) && draft.seed > 0 && draft.seed <= 4294967295 && !!draft.timezone.trim();
    const editLocked = locked || !active || active.running || active.cleanup_pending || !!active.error;
    async function saveName() {
        if (!renaming || !renameCurrent || locked || !renameValid) return;
        if (await act('browser_environment_rename', {id: renaming.id, name: renameValue.trim()}) && live.current) setRenaming(null);
    }
    function editConfig<K extends keyof BrowserConfig>(field: K, value: BrowserConfig[K]) {
        if (!active || !draft) return;
        const id = active.id, base = draft;
        setDrafts(old => ({...old, [id]: {...(old[id] ?? base), [field]: value}}));
    }
    async function saveConfig() {
        if (!active || !draft || editLocked || !dirty || !draftValid) return;
        const id = active.id;
        // Explicit UUID and the current server-side name; the snapshot returned by the command is the read-back.
        if (await act('browser_environment_save', {id, name: active.name, config: draft}) && live.current)
            setDrafts(old => withoutDraft(old, id));
    }
    async function create() {
        if (!creator || locked || !creatorValid) return;
        const previous = new Set(snapshot?.items.map(item => item.id));
        const next = await act('browser_environment_create', {name: creator.name.trim(), temporary: creator.temporary, config: null});
        if (!next || !live.current) return;
        const created = next.items.find(item => !previous.has(item.id));
        setCreator(null);
        if (created) select(created.id);
    }
    async function refresh() {
        if (await act('browser_environments_read')) await onRefresh?.();
    }
    function begin(temporary: boolean) { setError(''); setCreator({name: '', temporary}); }
    const running = (snapshot?.items.filter(item => item.running).length ?? 0) + (native.running ? 1 : 0);

    const renderCard = (item: EnvironmentSummary) => (<div key={item.id} role="listitem" className={`environment-card-frame${key === item.id ? ' selected' : ''}`}>
                        <article className={`identity-card browser-environment-card${item.running && item.runtime_status !== 'unknown' ? ' current' : ''}${background(item) ? ' environment-background' : ''}`}
                            aria-label={`${item.name} · ${status(item)}`} onClickCapture={() => select(item.id)}>
                            <div className="identity-card-head"><div className="identity-avatar" aria-hidden="true">{Array.from(item.name)[0]?.toUpperCase()}</div><span className="identity-live-badge">{status(item)}</span></div>
                            <h3 className="environment-card-name">
                                <button type="button" className="environment-card-choice" disabled={locked} aria-pressed={key === item.id}
                                    aria-label={t("选择环境 {v0}", {v0: item.name})} onClick={() => select(item.id)}>{item.name}</button>
                                {!item.daily && <button type="button" className="environment-card-rename" disabled={locked} aria-label={t("重命名环境 {v0}", {v0: item.name})}
                                    onClick={() => { setError(''); setRenaming(item); setRenameValue(item.name); }}>✎</button>}
                            </h3>
                            <div className="identity-badges"><span className="identity-status-badge">{t(item.brave ? "Brave 环境" : item.temporary ? "一次性环境" : "长期环境")}</span></div>
                            <div className="identity-account environment-config-summary">{item.source ? t(item.source) : item.brave ? t("沿用 Brave 原设置") : item.config.follow_machine ? t("语言与时区跟随机器") : `${item.config.language} · ${item.config.timezone}`}</div>
                            {background(item) && <p className="environment-runtime-hint" role="status">{t(item.runtime_status === 'residual' ? "相关后台进程未退出" : "未见窗口，进程仍在")}</p>}
                            {item.error && <p role="alert" className="environment-card-error" title={t(item.error)}>{t(item.error)}</p>}
                            <div className="identity-card-footer">
                                <Button size="small" variant="secondary" disabled={locked || (item.cleanup_pending && !item.running) || !!item.error || environmentDirty(item, drafts)} onClick={() => void act('browser_environment_launch', {id: item.id})}>{t("打开窗口")}</Button>
                                {!item.daily && <Button size="small" variant="ghost" disabled={locked || !item.running} loading={pending?.command === 'browser_environment_quit' && pending.id === item.id}
                                    onClick={() => void act('browser_environment_quit', {id: item.id})}>{t("关闭浏览器")}</Button>}
                            </div>
                        </article>
                    </div>);

    return <Panel className="system-card" aria-label={t("浏览器目录")}>
        <PanelHeader title={t("浏览器目录")} actions={<div className="environment-directory-status">{snapshot && <span role="status" className="system-card-number">{t("{v0} 个环境 · {v1} 运行中", {v0: snapshot.items.length + 1, v1: running})}</span>}<Button size="small" variant="secondary" disabled={locked} onClick={() => void refresh()}>{t("刷新状态")}</Button></div>}/>
        <div className="system-card-body">
            <div className="browser-actions">
                <Button variant="ink" disabled={locked || !snapshot} onClick={() => setAdding(true)}>{t("添加环境")}</Button>
                {snapshot?.items.some(item => item.temporary && item.cleanup_pending && !item.running) && <Button variant="ghost" disabled={locked} onClick={() => void act('browser_environments_cleanup')}>{t("清理一次性残留")}</Button>}
            </div>
            {error && <p role="alert" className="system-feedback error">{t(error)}</p>}
            {!!snapshot?.errors?.length && <div role="alert" className="system-feedback error">
                <p>{t("部分环境配置无法读取，资料已保留；其他环境仍可使用。")}</p>
                {snapshot.errors.map(item => <p key={item.id}><span>{t("未读取环境 {v0}", {v0: item.id})}</span><br/>{t(item.error)}</p>)}
            </div>}
            {!snapshot && <p role="status" className="system-hint">{t("环境状态尚未读取。")}</p>}
            <div className="environment-card-report-group" ref={cardGroup}>
                <svg className="environment-selection-outline" aria-hidden="true"><path d={outline}/></svg>
                <div className="environment-card-selector" role="list" aria-label={t("浏览器目录")}>
                    {snapshot?.items.filter(item => item.daily).map(renderCard)}
                    <div role="listitem" className={`environment-card-frame${key === NATIVE_TARGET ? ' selected' : ''}`}>
                        <article className={`identity-card browser-environment-card${native.running ? ' current' : ''}`}
                            aria-label={`${t("原生环境")} · ${nativeStatus}`} onClickCapture={() => select(NATIVE_TARGET)}>
                            <div className="identity-card-head"><div className="identity-avatar" aria-hidden="true">{Array.from(t("原生环境"))[0]?.toUpperCase()}</div><span className="identity-live-badge">{nativeStatus}</span></div>
                            <h3 className="environment-card-name">
                                <button type="button" className="environment-card-choice" disabled={locked} aria-pressed={key === NATIVE_TARGET}
                                    aria-label={t("选择环境 {v0}", {v0: t("原生环境")})} onClick={() => select(NATIVE_TARGET)}>{t("原生环境")}</button>
                            </h3>
                            {native.linked && <div className="identity-badges"><span className="identity-status-badge">{t("身份联动")}</span></div>}
                            {native.summary && <div className="identity-account environment-config-summary">{native.summary}</div>}
                            <div className="identity-card-footer">{native.footer}</div>
                        </article>
                    </div>
                    {snapshot?.items.filter(item => !item.daily).map(renderCard)}
                </div>
                <section className="environment-detail" aria-label={key === NATIVE_TARGET ? t("原生环境") : active?.name}>
                    {key === NATIVE_TARGET ? <>
                        <div className="environment-detail-heading"><h3>{t("原生环境")}</h3><span>{nativeStatus}</span></div>
                        {native.detail}
                    </> : active ? <div key={active.id}>
                        <div className="environment-detail-heading"><h3>{active.name}</h3><span>{status(active)}</span></div>
                        {active.source && <p className="system-hint">{t(active.source)}</p>}
                        {active.external_profile && <p className="system-hint">{active.external_profile}</p>}
                        {!draft ? <p className="system-hint">{t("此环境沿用 Brave 原设置，请在浏览器中配置。")}</p> : <div className="environment-settings">
                            <Panel className="system-card"><PanelHeader title={t("指纹与代理设置")}/><div className="system-card-body">
                                <label htmlFor="environment-seed">{t("稳定指纹种子")}</label>
                                <Input id="environment-seed" type="number" min={1} max={4294967295} value={draft.seed} disabled={editLocked} onChange={event => editConfig('seed', Number(event.currentTarget.value))}/>
                                <label htmlFor="environment-proxy">{t("浏览器代理")}</label>
                                <Input id="environment-proxy" placeholder={t("留空沿用当前网络")} value={draft.proxy} disabled={editLocked} onChange={event => editConfig('proxy', event.currentTarget.value)}/>
                                <label className="browser-toggle"><Checkbox checked={draft.follow_machine} disabled={editLocked} onCheckedChange={value => editConfig('follow_machine', value === true)}/>{t("语言与时区跟随机器")}</label>
                                {!draft.follow_machine && <>
                                    <label>{t("网页语言")}</label>
                                    <Select aria-label={t("浏览器语言")} value={draft.language} disabled={editLocked} onValueChange={value => { if (value) editConfig('language', value); }} items={languages.map(value => ({value, label: value}))}/>
                                    <label>{t("网页时区")}</label>
                                    <Select aria-label={t("浏览器时区")} value={draft.timezone} disabled={editLocked} onValueChange={value => { if (value) editConfig('timezone', value); }} items={[...new Set([draft.timezone, ...timezones])].map(value => ({value, label: value}))}/>
                                </>}
                                <label className="browser-toggle" title={t("仅控制 Canvas 与页面元素矩形；取消勾选仅请求种子扰动，不代表完整设备指纹。")}><Checkbox checked={draft.native_graphics} disabled={editLocked} onCheckedChange={value => editConfig('native_graphics', value === true)}/>{t("保留原始图形特征")}</label>
                                {dirty && <p className="system-hint" role="status">{t("先保存设置，再打开窗口。")}</p>}
                                <div className="system-card-actions">
                                    <Button variant="ink" size="small" disabled={editLocked || !dirty || !draftValid} loading={pending?.command === 'browser_environment_save' && pending.id === active.id} onClick={() => void saveConfig()}>{t("保存设置")}</Button>
                                    {dirty && <Button variant="ghost" size="small" disabled={locked} onClick={() => { const id = active.id; setDrafts(old => withoutDraft(old, id)); }}>{t("取消")}</Button>}
                                </div>
                            </div></Panel>
                        </div>}
                        {!active.daily && <div className="environment-danger">
                            <Button variant="ghost" size="small" disabled={locked || (!active.external_profile && (active.running || !!active.error))} onClick={() => { setAccepted(false); setRemoving(active); }}>{t(active.external_profile ? "解除引用" : "删除环境")}</Button>
                        </div>}
                    </div> : <p role="status" className="system-hint">{t("环境状态尚未读取。")}</p>}
                </section>
            </div>
        </div>

        <BrowserAddDialog open={adding} disabled={locked} onOpenChange={setAdding} onCreate={begin} onReference={async (name, path) => {
            const next = await act('browser_environment_reference', {name, path});
            if (next) { setAdding(false); select(next.items.find(item => item.external_profile === path)?.id ?? 'daily'); }
            return !!next;
        }}/>
        <AppDialog open={!!renaming} onOpenChange={open => { if (!open && !locked) setRenaming(null); }} title={t("重命名环境")} footer={<>
            <Button variant="secondary" disabled={locked} onClick={() => setRenaming(null)}>{t("取消")}</Button>
            <Button variant="ink" disabled={locked || !renameCurrent || !renameValid} loading={busy} onClick={() => void saveName()}>{t("保存")}</Button>
        </>}>
            <div className="system-card-body">
                <label htmlFor="environment-rename">{t("环境名称")}</label>
                <Input id="environment-rename" value={renameValue} disabled={locked} onChange={event => setRenameValue(event.currentTarget.value)}
                    onKeyDown={event => { if (event.key === 'Enter' && !event.nativeEvent.isComposing) { event.preventDefault(); void saveName(); } }}/>
                {!renameValid && <p className="system-hint">{t("环境名称须为 1–80 个字符且不能包含控制字符。")}</p>}
                {error && <p role="alert" className="system-feedback error">{t(error)}</p>}
                {renaming && !renameCurrent && <p role="status" className="system-hint">{t("环境状态已变化，暂时无法保存。请关闭编辑并刷新。")}</p>}
            </div>
        </AppDialog>

        <AppDialog open={!!creator} onOpenChange={open => { if (!open && !locked) setCreator(null); }} title={t(creator?.temporary ? "新建一次性环境" : "新建长期环境")} footer={<>
            <Button variant="secondary" disabled={locked} onClick={() => setCreator(null)}>{t("取消")}</Button>
            <Button variant="ink" disabled={locked || !creatorValid} loading={busy} onClick={() => void create()}>{t("创建并配置")}</Button>
        </>}>
            <div className="system-card-body">
                <label htmlFor="environment-name">{t("环境名称")}</label>
                <Input id="environment-name" value={creator?.name || ''} disabled={locked} onChange={event => { const name = event.currentTarget.value; setCreator(old => old ? {...old, name} : old); }}
                    onKeyDown={event => { if (event.key === 'Enter' && !event.nativeEvent.isComposing) { event.preventDefault(); void create(); } }}/>
                {error && <p role="alert" className="system-feedback error">{t(error)}</p>}
            </div>
        </AppDialog>

        <AppDialog alert className="identity-delete-alert" open={!!removing} onOpenChange={open => { if (!open && !locked) { setRemoving(null); setAccepted(false); } }} title={t("删除环境")} description={t(removing?.external_profile ? "仅解除 CCBaga 登记；不关闭浏览器，不删除磁盘资料。" : "永久删除所选环境的登录、密码、书签、历史、扩展、设置和缓存，不创建备份。")} footer={<>
            <Button variant="secondary" data-dialog-cancel disabled={locked} onClick={() => { setRemoving(null); setAccepted(false); }}>{t("取消")}</Button>
            <Button variant="ink" className="identity-delete-button" disabled={locked || !accepted || !removalCurrent || (!removalCurrent.external_profile && (removalCurrent.running || !!removalCurrent.error))} loading={busy} onClick={() => { if (!removing || !accepted || !removalCurrent || (!removalCurrent.external_profile && (removalCurrent.running || removalCurrent.error))) return; void act('browser_environment_delete', {id: removing.id}).then(ok => { if (ok && live.current) { setRemoving(null); setAccepted(false); } }); }}>{t(removing?.external_profile ? "解除引用" : "确认删除环境")}</Button>
        </>}>
            <div className="identity-confirm">
                <strong>{removing?.name}</strong>
                <p>{t("其他独立环境、身份联动资料和身份保险库不受影响。")}</p>
                <Checkbox checked={accepted} disabled={locked} onCheckedChange={value => setAccepted(value === true)} label={t("我知道此操作无法撤销")}/>
                {error && <p role="alert" className="system-feedback error">{t(error)}</p>}
            </div>
        </AppDialog>
    </Panel>;
}
