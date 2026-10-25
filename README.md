# bamboo

[![CI](https://github.com/xcvzmoon/bamboo/actions/workflows/ci.yaml/badge.svg)](https://github.com/xcvzmoon/bamboo/actions/workflows/ci.yaml)
[![Release](https://github.com/xcvzmoon/bamboo/actions/workflows/release.yaml/badge.svg)](https://github.com/xcvzmoon/bamboo/actions/workflows/release.yaml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE.md)

**Your next desktop app starts here.**

Build with Vue. Style with Tailwind. Add native capabilities with Rust. This template brings Nuxt 4 and Tauri 2 together with a themed app shell, UI components, and development tooling already connected, so you can get straight to your app.

[Use this template](https://github.com/xcvzmoon/bamboo/generate) · [Get started](#get-started) · [Choose a package manager](#use-bun-npm-or-yarn)

## What's inside

- **Nuxt 4, Vue 3, and TypeScript** for a familiar frontend workflow, configured as a client-side app for Tauri.
- **Tauri 2 and Rust** for the native application, with debug logging configured.
- **Tailwind CSS 4 and shadcn-vue** for styling and components you own. Button and ScrollArea components are included.
- **Light and dark themes** with a built-in theme toggle and a draggable top bar.
- **VueUse and Nuxt Icon** for composables and icons, with the Hugeicons collection installed.
- **Valibot** for schema validation, already used in the Nuxt configuration.
- **tauri-nuxt-dev** to start Nuxt and Tauri together with one development command.
- **Oxfmt, type-aware Oxlint, Clippy, and Lefthook** for formatting, linting, and Git checks.
- **GitHub Actions and genbumppush** for CI checks, builds, and tag-triggered GitHub releases.

The starter screen is deliberately small: a top bar, a theme toggle, and space for your interface.

## Get started

### 1. Install the prerequisites

- Node.js 26 and the pnpm version pinned in `package.json` under `devEngines.packageManager`, matching CI. See [other package managers](#use-bun-npm-or-yarn) if you prefer Bun, npm, or Yarn.
- Rust 1.90 or newer.
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system, including native build tools and system libraries.

Keep Node.js installed when using Bun too; the template's tooling includes Node-based commands.

### 2. Create your project

Choose [Use this template](https://github.com/xcvzmoon/bamboo/generate) on GitHub to create your own repository, then clone it. To try this starter directly:

```bash
git clone https://github.com/xcvzmoon/bamboo.git my-desktop-app
cd my-desktop-app
```

Copy `.env.example` to `.env` using your editor or terminal:

```bash
cp .env.example .env
```

On Windows PowerShell, use `Copy-Item .env.example .env`.

### 3. Install and launch

```bash
pnpm install
pnpm dev
```

A native app window opens with the starter interface. Edit `app/app.vue` to start building your screen; frontend changes reload during development.

The development port is `65432`. If you change it, keep `.env`, `tauri-nuxt-dev.config.ts`, and `build.devUrl` in `src-tauri/tauri.conf.json` aligned.

## Make it yours

- Build your interface in `app/app.vue` and add components under `app/components/`.
- Adjust theme colors and typography in `app/assets/css/tailwind.css`.
- Add shadcn-vue components with `pnpm add-component`, or `pnpm add-component dialog` for a specific component.
- Add native functionality in `src-tauri/src/lib.rs` and configure permissions in `src-tauri/capabilities/`.
- Set your app's product name, identifier, window title, and bundle settings in `src-tauri/tauri.conf.json`. Update the identifier-specific asset scope when changing the identifier.
- Update package metadata in `package.json` and `src-tauri/Cargo.toml`, the title in `app/components/Topbar.vue`, and the icons in `src-tauri/icons/`.

## Everyday commands

| Task                   | pnpm             | Bun                 | npm                 | Yarn                 |
| ---------------------- | ---------------- | ------------------- | ------------------- | -------------------- |
| Install dependencies   | `pnpm install`   | `bun install`       | `npm install`       | `yarn install`       |
| Start the desktop app  | `pnpm dev`       | `bun run dev`       | `npm run dev`       | `yarn run dev`       |
| Check formatting       | `pnpm check`     | `bun run check`     | `npm run check`     | `yarn run check`     |
| Lint web and Rust code | `pnpm lint`      | `bun run lint`      | `npm run lint`      | `yarn run lint`      |
| Typecheck              | `pnpm typecheck` | `bun run typecheck` | `npm run typecheck` | `yarn run typecheck` |
| Run Rust tests         | `pnpm test`      | `bun run test`      | `npm run test`      | `yarn run test`      |
| Build the desktop app  | `pnpm build`     | `bun run build`     | `npm run build`     | `yarn run build`     |

**Before using the Bun, npm, or Yarn columns, complete the setup below.** The repository ships with pnpm commands and CI; changing only the command you type does not replace those internal calls.

`fmt` formats web and Rust files. `validate` formats, lints, checks formatting, typechecks, and runs Rust tests; it does not build the app. The Rust test harness is configured, but the starter currently contains no tests.

## Use Bun, npm, or Yarn

pnpm is the default and the package manager used in CI. To adopt another manager in your copy of the template:

### 1. Replace the internal pnpm commands

In `package.json`, update `devEngines.packageManager` to your chosen manager and its exact installed version. Replace these two script values:

| Script        | Bun                        | npm                        | Yarn                        |
| ------------- | -------------------------- | -------------------------- | --------------------------- |
| `postinstall` | `bun run nuxt:postinstall` | `npm run nuxt:postinstall` | `yarn run nuxt:postinstall` |
| `typecheck`   | `bun run nuxt:typecheck`   | `npm run nuxt:typecheck`   | `yarn run nuxt:typecheck`   |

In `src-tauri/tauri.conf.json`, update the build hooks:

| Setting                    | Bun                     | npm                     | Yarn                     |
| -------------------------- | ----------------------- | ----------------------- | ------------------------ |
| `build.beforeDevCommand`   | `bun run dev`           | `npm run dev`           | `yarn run dev`           |
| `build.beforeBuildCommand` | `bun run nuxt:generate` | `npm run nuxt:generate` | `yarn run nuxt:generate` |

Replace each `pnpm <script>` call in `lefthook.yaml` with `bun run <script>`, `npm run <script>`, or `yarn run <script>`. Preserve the staged-file arguments and existing exclusions.

### 2. Use one lockfile

Remove the template's `pnpm-lock.yaml` and any existing `node_modules` directory before installing with your chosen manager. Commit the resulting `bun.lock`, `package-lock.json`, or `yarn.lock` in your own project. Once you have transferred the build-script policy described below, remove the pnpm-only `pnpm-workspace.yaml` too.

`tauri-nuxt-dev` detects the package manager from `packageManager` in `package.json`, then lockfiles, then the invoking package manager. Keeping one lockfile avoids ambiguous detection. If you set `packageManager`, use your chosen manager and its exact installed version.

### 3. Apply the manager-specific setup

#### Bun

The pnpm configuration allows dependency build scripts for `esbuild`, `lefthook`, and `vue-demi`. To carry that policy over to Bun, add this top-level field to `package.json` before installing:

```json
{
  "trustedDependencies": ["esbuild", "lefthook", "vue-demi"]
}
```

Then run:

```bash
bun install
bun run dev
```

See [Bun's lifecycle script documentation](https://bun.sh/docs/pm/lifecycle) for how trusted dependencies work. These commands use Bun as the package manager and script runner; they do not force every tool to use the Bun runtime.

#### npm

```bash
npm install
npm run dev
```

npm normally runs dependency install scripts, so the pnpm build-script allowlist is not transferred automatically.

#### Yarn

For modern Yarn, create `.yarnrc.yml` with the following setting. This keeps a `node_modules` layout that matches the template's Tauri CLI schema path:

```yaml
nodeLinker: node-modules
```

Then run:

```bash
yarn install
yarn run dev
```

See [Yarn's configuration reference](https://yarnpkg.com/configuration/yarnrc#nodeLinker) for the linker setting.

### 4. Update CI in your project

The shared action at `.github/actions/setup-project/action.yaml` uses `pnpm/setup` to read the pnpm version from `devEngines.packageManager` in `package.json` and provide Node.js, dependency installation, and caching. Replace that step with setup and installation for your chosen manager, keeping Node.js 26 and the Tauri system dependencies.

Update the pnpm commands in `.github/workflows/ci.yaml` and `.github/workflows/release.yaml` too. In the release workflow, replace `pnpm exec genbumppush` with `bunx genbumppush`, `npm exec -- genbumppush`, or `yarn exec genbumppush`.

The alternative-manager instructions describe how to adapt your copy; the supplied CI currently verifies pnpm only.

## Build for distribution

```bash
pnpm build
```

Tauri generates the static Nuxt frontend, loads it from `dist`, and builds the native application and bundles. Look for installers and bundles under `src-tauri/target/release/bundle/`.

Build on the target operating system for the usual desktop distribution workflow. App signing, notarization, and store submission need your own credentials and configuration. The included release workflow validates the project and creates or syncs a GitHub release; it does not upload a full set of desktop installers across operating systems.

## Project layout

```text
app/
  app.vue                 Starter screen
  assets/css/tailwind.css  Theme tokens and global styles
  components/             App components and shadcn-vue UI
  modules/                Local Nuxt modules
src-tauri/
  src/                    Rust application
  capabilities/           Tauri permissions
  icons/                  Application icons
  tauri.conf.json         Window, build, and bundle settings
.github/
  actions/setup-project/  Shared CI environment setup
  workflows/              CI and release automation
```

## Contributing and security

Read [CONTRIBUTING.md](./CONTRIBUTING.md) before sending a pull request and [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md) when participating in the project. Report vulnerabilities using [SECURITY.md](./SECURITY.md).

## License

MIT. Use it for your next project. See [LICENSE.md](./LICENSE.md).
