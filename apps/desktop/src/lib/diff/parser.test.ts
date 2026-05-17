import { describe, expect, it } from 'vitest';

import { flattenDiffRows, parsePatch } from '$lib/diff/parser';

const PATCH = `diff --git a/src/file.ts b/src/file.ts
index 0000000..1111111 100644
--- a/src/file.ts
+++ b/src/file.ts
@@ -1,2 +1,3 @@
 line 1
-line 2
+line 2 changed
+line 3
`;

describe('parsePatch', () => {
  it('parses files hunks and line numbers', () => {
    const files = parsePatch(PATCH);
    expect(files).toHaveLength(1);
    expect(files[0]?.path).toBe('src/file.ts');
    expect(files[0]?.hunks).toHaveLength(1);
    const lines = files[0]?.hunks[0]?.lines ?? [];
    expect(lines).toHaveLength(4);
    expect(lines[1]).toMatchObject({ kind: 'del', leftLine: 2, rightLine: null, text: 'line 2' });
    expect(lines[2]).toMatchObject({
      kind: 'add',
      leftLine: null,
      rightLine: 2,
      text: 'line 2 changed'
    });
    expect(lines[3]).toMatchObject({ kind: 'add', leftLine: null, rightLine: 3, text: 'line 3' });
  });

  it('flattens file and hunk structure for virtualization', () => {
    const rows = flattenDiffRows(parsePatch(PATCH));
    expect(rows[0]?.kind).toBe('file');
    expect(rows[1]?.kind).toBe('hunk');
    expect(rows.filter((row) => row.kind === 'line')).toHaveLength(4);
  });
});
