import {expect, test} from 'bun:test';
import {environmentDirty, environmentDraft, resolveBrowserTarget, targetKey, withoutDraft, type EnvironmentSummary} from './browser-model';

const item = (id: string, extra: Partial<EnvironmentSummary> = {}): EnvironmentSummary => ({
    id, name: id, temporary: false, running: false, cleanup_pending: false, error: null,
    config: {seed: 1, proxy: '', follow_machine: true, language: 'en-US', timezone: 'UTC', native_graphics: true},
    ...extra,
});
const A = 'a'.repeat(32), B = 'b'.repeat(32);

test('first use selects daily Brave even when another environment is running', () => {
    expect(resolveBrowserTarget([item(A, {running: true, runtime_status: 'open'})], null)).toEqual({kind: 'environment',id:'daily'});
    expect(resolveBrowserTarget(null, 'native')).toEqual({kind: 'native'});
});

test('a stored UUID survives rename, refresh and reordering, not array position', () => {
    expect(resolveBrowserTarget([item(B), item(A, {name: 'renamed'})], A)).toEqual({kind: 'environment', id: A});
    expect(resolveBrowserTarget([item(A), item(B)], B)).toEqual({kind: 'environment', id: B});
});

test('an unread list holds the stored UUID instead of showing another target', () => {
    expect(resolveBrowserTarget(null, A)).toBeNull();
    expect(targetKey(null)).toBeNull();
});

test('a removed or unlisted UUID falls back to daily Brave, never to a neighbouring environment', () => {
    expect(resolveBrowserTarget([item(B)], A)).toEqual({kind: 'environment',id:'daily'});
    expect(resolveBrowserTarget([], A)).toEqual({kind: 'environment',id:'daily'});
    expect(targetKey({kind: 'native'})).toBe('native');
    expect(targetKey({kind: 'environment', id: A})).toBe(A);
});

test('an unsaved draft gates only its own card and save clears only that UUID', () => {
    const a = item(A), b = item(B);
    const drafts = {[A]: {...a.config, seed: 2}};
    expect(environmentDirty(a, drafts)).toBe(true);
    expect(environmentDirty(b, drafts)).toBe(false);
    const both = {...drafts, [B]: {...b.config, proxy: 'http://127.0.0.1:1'}};
    const savedB = withoutDraft(both, B);
    expect(environmentDirty(a, savedB)).toBe(true);
    expect(environmentDirty(b, savedB)).toBe(false);
    // Read-back equal to the draft is clean even before the draft is dropped.
    expect(environmentDirty({...a, config: {...a.config, seed: 2}}, drafts)).toBe(false);
});

test('Brave never forms a draft from its placeholder config', () => {
    const brave = item(A, {brave: true});
    expect(environmentDraft(brave, {[A]: {...brave.config, seed: 9}})).toBeNull();
    expect(environmentDirty(brave, {[A]: {...brave.config, seed: 9}})).toBe(false);
});

test('daily default waits for readback, native choice remains explicit', () => {
    expect(resolveBrowserTarget(null,null)).toBeNull();
    expect(resolveBrowserTarget([item('daily',{daily:true,brave:true})],null)).toEqual({kind:'environment',id:'daily'});
    expect(resolveBrowserTarget([], 'native')).toEqual({kind:'native'});
});
