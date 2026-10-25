# Contributing to bamboo

Thanks for contributing. This guide covers setup, checks, and pull requests. Please also follow the [Code of Conduct](./CODE_OF_CONDUCT.md).

## Ways to contribute

- Report bugs through the [bug report template](https://github.com/xcvzmoon/bamboo/issues/new/choose).
- Suggest features through the [feature request template](https://github.com/xcvzmoon/bamboo/issues/new/choose).
- Report vulnerabilities privately using the process in [SECURITY.md](./SECURITY.md).
- Send focused code or documentation changes. For larger changes, open an issue first to discuss the approach.

## Development setup

Follow the [README](./README.md) for prerequisites. Then install dependencies and start the app:

```bash
cp .env.example .env
pnpm install
pnpm dev
```

The `.env.example` file sets the Nuxt port used by the Tauri development server. `pnpm dev` starts the Nuxt and Tauri development workflow.

## Coding standards

Read [AGENTS.md](./AGENTS.md) for the full project conventions. Keep changes focused, use the existing TypeScript and Rust patterns, and update tests when behavior changes.

Lefthook formats and lints matching staged files on commit. Run the project checks before opening a pull request:

```bash
pnpm check
pnpm typecheck
pnpm test
pnpm build
pnpm lint
```

`pnpm check` checks formatting. `pnpm lint` runs Oxlint and Clippy. `pnpm test` runs the Rust test suite. `pnpm build` generates the Nuxt frontend and builds the Tauri app.

## Commit messages and pull requests

Use a [Conventional Commit](https://www.conventionalcommits.org/) title, such as `fix: handle a failed Tauri command` or `docs: update setup instructions`.

1. Make the smallest change that solves the issue.
2. Run the checks above and describe any checks you could not run.
3. Open a pull request against `main` using the pull request template.
4. Address review comments and keep the pull request title accurate.

## License

Contributions are licensed under the project's [MIT License](./LICENSE.md).
