module.exports = {
  root: true,
  env: {
    browser: true,
    es2022: true,
    node: true
  },
  parser: '@typescript-eslint/parser',
  parserOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module'
  },
  plugins: ['@typescript-eslint'],
  extends: ['eslint:recommended', 'plugin:@typescript-eslint/recommended', 'prettier'],
  overrides: [
    {
      files: ['src/**/*.{ts,tsx,js,jsx}'],
      rules: {
        'no-restricted-imports': [
          'error',
          {
            paths: [
              { name: 'marked', message: 'Renderer markdown must go through IPC/comrak.' },
              { name: 'markdown-it', message: 'Renderer markdown must go through IPC/comrak.' },
              { name: 'remark', message: 'Renderer markdown must go through IPC/comrak.' },
              { name: 'remark-parse', message: 'Renderer markdown must go through IPC/comrak.' },
              { name: 'remark-gfm', message: 'Renderer markdown must go through IPC/comrak.' },
              { name: 'showdown', message: 'Renderer markdown must go through IPC/comrak.' }
            ],
            patterns: ['remark-*', '@remark/*', 'unified']
          }
        ],
        'no-restricted-syntax': [
          'error',
          {
            selector: "CallExpression[callee.name='fetch']",
            message: 'Renderer network calls must go through typed IPC commands.'
          }
        ]
      }
    }
  ],
  ignorePatterns: [
    '.svelte-kit/',
    'build/',
    'dist/',
    'src-tauri/target/',
    '**/*.svelte',
    'src/lib/ipc/bindings.ts'
  ]
};
