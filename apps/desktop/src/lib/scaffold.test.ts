import { describe, expect, it } from 'vitest';

describe('desktop scaffold', () => {
  it('keeps placeholder gate alive', () => {
    expect('pr-cockpit').toContain('cockpit');
  });
});
