# Conditional toolchain policy

Apply a section only when repository files show that the corresponding tool or workflow is present.

## Oxlint

- Enforce type-only imports with `typescript/consistent-type-imports`.
- Treat non-null assertions as warnings at minimum and prefer code changes that make the value provably present.
- Keep `no-console` aligned with the logging policy: reject `console.log`; permit `warn`, `error`, and `info` where appropriate.
- Use type-aware rules to catch unsafe promises, assertions, narrowing, and imports. Do not silence them with casts.

Follow the repository's exact severity and override configuration when it is stricter.

## Release conventions

Commit titles follow Conventional Commits when changelogen parses history. Preserve the repository's release topology rather than assuming one host:

- changelogen writes `CHANGELOG.md` and creates or supports `v*` tags;
- GitLab release creation uses its API and a redacted `GITLAB_TOKEN` when configured;
- GitHub Actions may turn pushed tags into GitHub releases;
- some repositories may intentionally target only one host or use another version/tag scheme.

Use the `release-script-generator` skill when the user asks to create or replace a release script. Merely editing code does not authorize a release, tag, push, or hosted release.

## Lefthook

Preserve the local hook contract. The common policy is:

- pre-commit formats and lints staged files;
- pre-push runs `check`, `typecheck`, and `test`.

Fix failures instead of using `--no-verify`. If a hook is broken independently of the change, identify the failing command and report it.

## Dependabot and TypeScript

When Dependabot configuration intentionally ignores TypeScript for toolchain compatibility, do not bump TypeScript manually or remove the ignore rule. Check compatibility with golar, Oxlint, native TypeScript previews, framework type checkers, and the active Vite+ release before proposing a coordinated upgrade.

## Curated repository skills

Formatting and linting may intentionally ignore `.agents/skills/` because those files are vendored or independently managed. Do not reformat that directory as part of ordinary repository checks.

Before work in a covered domain, read the relevant local skill, such as `unslop`, `tdd`, schema-validation guidance, Vue/Pinia guidance, VueUse, Vitest, or code review. Local skill instructions remain subordinate to explicit user and repository instructions.
