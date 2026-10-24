/**
 * [INPUT]: 原生浏览器配置、含有效区域国家的机器快照和起始页实际观测。
 * [OUTPUT]: 不含令牌的身份联动观测与独立环境摘要；原生/UUID 显式选择目标，独立环境不包含实测报告，三平台设备核验保持原契约。
 * [POS]: 浏览器页面与宿主的边界，不持有凭据或执行清理。
 * [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
 */
export type BrowserConfig={seed:number;proxy:string;follow_machine:boolean;language:string;timezone:string;native_graphics:boolean};
export type Machine={platform:string;version:string;cores:number;timezone:string;languages:string[];locale:string;locale_country:string;location_enabled:boolean|null;timezones:string[]};
export type Exit={ip:string;country:string;city:string;timezone:string;isp:string};
export type Report={language:string;languages:string[];timezone:string;platform:string;cores:number;user_agent:string;platform_version:string;browser_version:string;geolocation:string;exit:Exit|null;error:string};
export type BrowserIdentity={linked:boolean;status:'linked'|'draft'|'empty'|'unlinked'|'mismatch'|'unknown';name:string|null;email:string|null;organization:string|null};
export type BrowserContext={identity?:BrowserIdentity;ui_language:'zh'|'en';exit_services:string[];region:{id:string;label:string;country:string;language:string;locale:string;timezone:string}|null;config:BrowserConfig;machine:Machine;engine_version:string};
export type BrowserState=BrowserContext&{engine_ready:boolean;running:boolean;profile:string;identity_linked?:boolean;cache:{path:string;bytes:number;login:boolean}[];cache_error:string;report:Report|null;report_received_at:number|null;machine_exit:Exit|null;checks:{title:string;status:'ok'|'mismatch'|'pending';detail:string}[]};
export type DefaultBrowser={is_default:boolean;http:string;https:string;installed:boolean;app:string};
export type EnvironmentRuntimeStatus='stopped'|'starting'|'closing'|'checking'|'open'|'background'|'residual'|'running'|'unknown';
export type EnvironmentSummary={runtime_status?:EnvironmentRuntimeStatus;process_count?:number|null;visible_window?:boolean|null;brave?:boolean;id:string;name:string;temporary:boolean;running:boolean;cleanup_pending:boolean;error:string|null;config:BrowserConfig};
export type EnvironmentSnapshot={items:EnvironmentSummary[];errors:{id:string;error:string}[]};

export function matchesDevice(machine: Pick<Machine, 'platform'|'version'|'cores'>, report: Pick<Report, 'platform'|'platform_version'|'cores'>) {
    const platform=report.platform.toLowerCase();
    const matched=machine.platform==='windows' ? platform==='win32' : platform.includes(machine.platform==='linux'?'linux':'mac');
    return matched && report.cores===machine.cores && (machine.platform==='linux'||report.platform_version===machine.version);
}

/** Explicit selection target: the original identity-linked browser or one registered environment UUID. */
export type BrowserTarget={kind:'native'}|{kind:'environment';id:string};
export const NATIVE_TARGET='native';

/**
 * Resolve the stored choice without guessing: first use and removed UUIDs fall back to the native browser;
 * a stored UUID is held (null) until the environment list is read, so the detail never shows another target.
 */
export function resolveBrowserTarget(items: readonly EnvironmentSummary[] | null, stored: string | null): BrowserTarget | null {
    if (!stored || stored === NATIVE_TARGET) return {kind:'native'};
    if (!items) return null;
    return items.some(item => item.id === stored) ? {kind:'environment', id: stored} : {kind:'native'};
}

export function targetKey(target: BrowserTarget | null) {
    return target ? target.kind === 'native' ? NATIVE_TARGET : target.id : null;
}

/** Unsaved edits keyed by environment UUID. Brave keeps its own settings, so its placeholder config never forms a draft. */
export type EnvironmentDrafts=Readonly<Record<string,BrowserConfig>>;
export function environmentDraft(item: EnvironmentSummary, drafts: EnvironmentDrafts): BrowserConfig | null {
    return item.brave ? null : drafts[item.id] ?? item.config;
}
export function environmentDirty(item: EnvironmentSummary, drafts: EnvironmentDrafts) {
    const draft = environmentDraft(item, drafts);
    return !!draft && JSON.stringify(draft) !== JSON.stringify(item.config);
}
export function withoutDraft(drafts: EnvironmentDrafts, id: string): EnvironmentDrafts {
    const next = {...drafts};
    delete next[id];
    return next;
}
