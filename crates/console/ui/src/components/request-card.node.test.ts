import { describe, it, expect } from 'vitest';
import { splitPlan } from './request-card.js';

describe('splitPlan', () => {
  it('passes plain text through with no contracts', () => {
    expect(splitPlan('Build the portal.')).toEqual({ plan: 'Build the portal.', contracts: [] });
  });

  it('splits the injected upstream contracts into sections', () => {
    const text = 'Build the portal.\n\n## Upstream contracts (landed; build on these)\n\n### backend/be-1\nrange abc..def\nGET /x\n\n### runner/run-2\n(closed; no contract artifact)\n';
    const { plan, contracts } = splitPlan(text);
    expect(plan).toBe('Build the portal.');
    expect(contracts.map((c) => c.need)).toEqual(['backend/be-1', 'runner/run-2']);
    expect(contracts[0]?.text).toContain('range abc..def');
    expect(contracts[1]?.text).toBe('(closed; no contract artifact)');
  });
});
