import { defineConfig } from 'oxfmt';

export default defineConfig({
  sortPackageJson: false,
  sortImports: {
    groups: [
      'type-import',
      'type-internal',
      ['type-parent', 'type-sibling', 'type-index'],
      ['value-builtin', 'value-external'],
      'value-internal',
      ['value-parent', 'value-sibling', 'value-index'],
      'unknown',
    ],
    newlinesBetween: false,
  },
  sortTailwindcss: {
    stylesheets: 'app/assets/css/tailwind.css',
    attributes: ['class'],
    functions: ['clsx', 'cn'],
    preserveWhitespace: true,
  },
  singleQuote: true,
  singleAttributePerLine: true,
  vueIndentScriptAndStyle: true,
  ignorePatterns: [
    'CHANGELOG.md',
    '.agents/**',
    'app/components/ui/**',
    'tools/oxlint/anti-slop/**',
    'tools/oxlint/custom/**',
  ],
});
