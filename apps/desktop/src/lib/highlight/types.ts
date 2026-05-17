export type HighlightToken = {
  start: number;
  end: number;
  kind: string;
};

export type HighlightLineTokens = {
  line: number;
  tokens: HighlightToken[];
};

export type HighlightResponse = {
  language: string;
  contentHash: string;
  lines: HighlightLineTokens[];
};
