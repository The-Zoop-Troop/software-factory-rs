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
