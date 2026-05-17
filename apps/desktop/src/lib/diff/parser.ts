export type DiffLineKind = 'context' | 'add' | 'del';

export type ParsedDiffLine = {
  kind: DiffLineKind;
  text: string;
  leftLine: number | null;
  rightLine: number | null;
};

export type ParsedDiffHunk = {
  header: string;
  oldStart: number;
  newStart: number;
  lines: ParsedDiffLine[];
};

export type ParsedDiffFile = {
  oldPath: string;
  newPath: string;
  path: string;
  hunks: ParsedDiffHunk[];
};

export type DiffRow =
  | {
      id: string;
      kind: 'file';
      file: ParsedDiffFile;
    }
  | {
      id: string;
      kind: 'hunk';
      file: ParsedDiffFile;
      hunk: ParsedDiffHunk;
    }
  | {
      id: string;
      kind: 'line';
      file: ParsedDiffFile;
      hunk: ParsedDiffHunk;
      line: ParsedDiffLine;
      lineIndex: number;
    };

export function parsePatch(patch: string): ParsedDiffFile[] {
  const lines = patch.split('\n');
  const files: ParsedDiffFile[] = [];
  let file: ParsedDiffFile | null = null;
  let hunk: ParsedDiffHunk | null = null;
  let oldCursor = 0;
  let newCursor = 0;

  const commitFile = (): void => {
    if (file) {
      if (!file.path) {
        file.path = file.newPath.replace(/^b\//, '') || file.oldPath.replace(/^a\//, '');
      }
      files.push(file);
    }
    file = null;
    hunk = null;
  };

  for (const line of lines) {
    if (line.startsWith('diff --git ')) {
      commitFile();
      const parts = line.split(' ');
      const oldPath = parts[2]?.replace(/^a\//, '') ?? '';
      const newPath = parts[3]?.replace(/^b\//, '') ?? oldPath;
      file = {
        oldPath,
        newPath,
        path: newPath || oldPath,
        hunks: []
      };
      continue;
    }
    if (!file) {
      continue;
    }
    if (line.startsWith('--- ')) {
      file.oldPath = line.slice(4).replace(/^a\//, '');
      if (!file.path) {
        file.path = file.oldPath;
      }
      continue;
    }
    if (line.startsWith('+++ ')) {
      file.newPath = line.slice(4).replace(/^b\//, '');
      file.path = file.newPath || file.oldPath;
      continue;
    }
    if (line.startsWith('@@ ')) {
      const match = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
      if (!match) {
        continue;
      }
      oldCursor = Number(match[1]);
      newCursor = Number(match[2]);
      hunk = {
        header: line,
        oldStart: oldCursor,
        newStart: newCursor,
        lines: []
      };
      file.hunks.push(hunk);
      continue;
    }
    if (!hunk) {
      continue;
    }
    if (line.startsWith('+') && !line.startsWith('+++')) {
      hunk.lines.push({
        kind: 'add',
        text: line.slice(1),
        leftLine: null,
        rightLine: newCursor
      });
      newCursor += 1;
      continue;
    }
    if (line.startsWith('-') && !line.startsWith('---')) {
      hunk.lines.push({
        kind: 'del',
        text: line.slice(1),
        leftLine: oldCursor,
        rightLine: null
      });
      oldCursor += 1;
      continue;
    }
    if (line.startsWith(' ')) {
      hunk.lines.push({
        kind: 'context',
        text: line.slice(1),
        leftLine: oldCursor,
        rightLine: newCursor
      });
      oldCursor += 1;
      newCursor += 1;
    }
  }

  commitFile();
  return files;
}

export function flattenDiffRows(files: ParsedDiffFile[]): DiffRow[] {
  const rows: DiffRow[] = [];
  for (const file of files) {
    rows.push({
      id: `file:${file.path}`,
      kind: 'file',
      file
    });
    for (const hunk of file.hunks) {
      rows.push({
        id: `hunk:${file.path}:${hunk.header}`,
        kind: 'hunk',
        file,
        hunk
      });
      hunk.lines.forEach((line, lineIndex) => {
        rows.push({
          id: `line:${file.path}:${hunk.header}:${lineIndex}`,
          kind: 'line',
          file,
          hunk,
          line,
          lineIndex
        });
      });
    }
  }
  return rows;
}
