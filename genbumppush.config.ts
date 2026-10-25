import { defineConfig } from 'genbumppush';

export default defineConfig({
  release: 'patch',
  files: [
    'package.json',
    'src-tauri/tauri.conf.json',
    'src-tauri/Cargo.toml',
    'src-tauri/Cargo.lock',
  ],
  hooks: {
    before: ['pnpm validate'],
  },
  github: {
    enabled: true,
  },
});
