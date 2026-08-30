import { describe, it, expect, beforeEach } from 'vitest';
import { Schema } from 'effect';
import { RigName, Task, type TaskState } from '../core/schema.js';
import { attentionCount, reset as resetRigs, rigs, setTasks, summaries } from './rigs.js';
import { dismiss, notices, notify, reset as resetNotices } from './notices.js';
import { loadToken, reset as resetSession, saveToken, token } from './session.js';

const rig = Schema.decodeSync(RigName)('toy');
const task = (id: string, kind: string, state: TaskState) =>
  Schema.decodeSync(Task)({ id, contextId: id, status: { state, timestamp: 't' }, metadata: { factory: { kind } } });

beforeEach(() => { resetRigs(); resetNotices(); resetSession(); });

describe('rig summaries', () => {
  it('count epics, working, attention and done per rig', () => {
    rigs.set([rig]);
    setTasks(rig, [task('e1', 'epic', 'TASK_STATE_WORKING'), task('e2', 'epic', 'TASK_STATE_COMPLETED'), task('i1', 'incident', 'TASK_STATE_INPUT_REQUIRED')]);
    expect(summaries.get()).toEqual([{ rig: 'toy', epics: 2, working: 1, attention: 1, done: 1 }]);
    expect(attentionCount.get()).toBe(1);
  });
});

describe('notices', () => {
  it('append and dismiss', () => {
    const id = notify('info', 'hi', 'there');
    notify('danger', 'bad');
    expect(notices.get().length).toBe(2);
    dismiss(id);
    expect(notices.get()[0]?.title).toBe('bad');
  });
});

describe('session', () => {
  it('persists the token when storage exists', () => {
    saveToken('abc');
    expect(token.get()).toBe('abc');
    expect(loadToken()).toBe(typeof localStorage === 'undefined' ? '' : 'abc');
  });
});

describe('attention and epic views', () => {
  it('collects attention items across rigs and finds tasks', async () => {
    const { attentionItems, taskById } = await import('./rigs.js');
    const api = Schema.decodeSync(RigName)('api');
    rigs.set([rig, api]);
    setTasks(rig, [task('e1', 'epic', 'TASK_STATE_INPUT_REQUIRED'), task('i1', 'incident', 'TASK_STATE_INPUT_REQUIRED')]);
    setTasks(api, [task('q1', 'question', 'TASK_STATE_INPUT_REQUIRED'), task('r1', 'plan_request', 'TASK_STATE_INPUT_REQUIRED')]);
    expect(attentionItems.get().map((i) => `${i.rig}/${i.task.id}`)).toEqual(['toy/i1', 'api/q1']);
    expect(taskById('toy', 'e1')?.id).toBe('e1');
    expect(taskById('toy', 'zz')).toBeUndefined();
  });

  it('filters events per epic and lists alert deliveries', async () => {
    const ev = await import('./events.js');
    ev.reset();
    const mk = (bead: string | null, kind: string, extra: Record<string, unknown> = {}) => ({ rig: 'toy', cursor: 1, record: { at: 1, actor: 'x', bead, kind, ...extra } });
    ev.push(mk('ep-1.2', 'claimed'));
    ev.push(mk('ep-1', 'epic_closed'));
    ev.push(mk('ep-10.1', 'claimed'));
    ev.push(mk(null, 'remote', { action: 'alert', detail: 'ep-1 done' }));
    ev.push(mk(null, 'remote', { action: 'alert-failed', detail: 'boom' }));
    ev.push(mk(null, 'remote', { action: 'plan', detail: 'x' }));
    expect(ev.forEpic('toy', 'ep-1').get().map((f) => f.record.bead)).toEqual(['ep-1.2', 'ep-1']);
    expect(ev.alerts.get().map((f) => f.record['action'])).toEqual(['alert-failed', 'alert']);
    expect(ev.str({ a: 1 })).toBe('{"a":1}');
    expect(ev.str(null)).toBe('');
  });
});
