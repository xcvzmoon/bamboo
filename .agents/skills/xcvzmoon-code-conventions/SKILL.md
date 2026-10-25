---
name: xcvzmoon-code-conventions
description: Apply xcvzmoon's coding, editing, TypeScript, naming, validation, and agent-operation conventions while implementing, refactoring, reviewing, or documenting code in a repository that adopts these rules. Use broadly for coding work in such repositories; repository and explicit user instructions take precedence.
---

# xcvzmoon Code Conventions

Apply these rules throughout the requested change. Explicit user instructions and more specific repository instructions take precedence when they conflict.

## Default quality bar

Treat every request to create, plan, fix, implement, design, or deliver a result as implicitly requiring a professional, production-ready, and enterprise-grade outcome, even when the user does not repeat those words.

Interpret that quality bar as correctness, reliability, security, maintainability, operational clarity, appropriate validation, and complete handling of realistic failure modes. It does not authorize expanding scope, adding speculative infrastructure, introducing unnecessary abstraction, or replacing a small correct solution with a large framework. Apply the quality bar proportionally to the requested task and the repository's actual deployment context.

## Editing discipline

- Make the smallest correct change and keep the diff focused.
- Leave correct existing code, comments, and documentation in place.
- Do not polish unrelated code, broadly rename the codebase, or expand the task into a repository-wide refactor unless the requested outcome requires it.
- For production-sensitive code, prioritize reliability over clever abstractions.
- Choose the simplest implementation that fully meets current requirements. Avoid speculative abstraction, configuration, and indirection.
- Grow the system through working end-to-end layers; do not trade a working product for unfinished complexity.
- Keep concerns separate and components modular.
- Prefer established, maintained libraries when they reduce total complexity or improve reliability. Check existing dependencies, documentation, and types before adding a package or reimplementing a capability.
- Make durable architectural decisions. Do not introduce a known temporary workaround intended to be replaced later.
- If a workaround needs a paragraph-long justification, replace it with a sound implementation.

Do not preserve backward compatibility unless the user or repository contract requires it. When compatibility is not required, remove obsolete paths instead of adding fallbacks, migrations, aliases, or parallel implementations.

## Formatting and functions

- Match the surrounding file and configured formatter; do not hand-style a competing layout.
- Prefer `function name()` for named top-level functions and helpers.
- Keep callbacks as arrow functions, such as `items.map((item) => item.id)`.
- When only one or two non-conflicting properties are used from an iterated value, prefer parameter destructuring.
- Prefer functions over classes. Leave correct existing classes in place unless changing the design is necessary.
- Use `import type` for imports used only as types.

## Types and naming

- Prefer `type` over `interface`. Use an interface when declaration merging or another interface-specific capability is required.
- Avoid `any`. Use `unknown` and narrow it at a real boundary, but do not introduce a generic `isRecord` helper as a substitute for understanding the shape.
- Add explicit return types to exported functions and non-trivial helpers.
- Use string-literal unions for small state sets such as `'ok' | 'error'`.
- Keep generics minimal and purposeful, and reuse existing helper types before inventing new ones.
- Use descriptive names. Prefer `item`, `entry`, `record`, or `status` over abbreviated iterable variables.
- Reserve one-letter names for conventional indexes or equally established mathematical contexts.
- Avoid non-null assertions. Narrow, validate, restructure, or return early instead.

## Validation and failures

- Validate once at system boundaries rather than repeatedly in inner layers.
- Use the repository's existing Standard Schema-compliant validator for environment, form, and request validation. When selecting a validator for new code, prefer a maintained library that implements Standard Schema and fits the target runtime.
- Name schema values in camel case, such as `userSchema`, rather than Pascal case.
- Prefer the validator's composable schema primitives, built-in validation actions, and reusable transforms over manual parsing and ad hoc validation.
- Never throw raw strings.
- Give configuration and application failures stable error codes when callers or operators need to classify them.
- Catch infrastructure failures where graceful degradation is expected; otherwise preserve the cause and let the appropriate boundary handle it.
- Clean up temporary resources in `finally` blocks.

## Logging and prose

Avoid `console.log`. In standalone scripts and code outside the Nuxt runtime, use `consola` when it is already available or appropriate to add. Direct `console.warn`, `console.error`, and `console.info` are acceptable when they match the local runtime and policy.

The `/unslop` skill is required before generating or editing user-facing prose, documentation, JSDoc, TSDoc, or code comments. Check the available skill catalog and repository-local skills before starting that work. If `unslop` is present, read its complete `SKILL.md` and apply it.

If `unslop` is absent, stop before writing the affected text and ask the user to install it from `https://github.com/poteto/noodle` with the active package manager:

- npm: `npx skills add https://github.com/poteto/noodle --skill unslop`
- pnpm: `pnpm dlx skills add https://github.com/poteto/noodle --skill unslop`
- Bun: `bunx skills add https://github.com/poteto/noodle --skill unslop`
- Yarn: `yarn dlx skills add https://github.com/poteto/noodle --skill unslop`

Use a package manager explicitly specified by the user; otherwise infer the active manager from `packageManager`, `devEngines.packageManager`, the lockfile, and repository instructions. Do not default to `npx` when another manager is active. Do not run the installation without the user's confirmation. After installation, read the installed skill and resume the original work. Do not silently substitute an imitation of `unslop` or claim to have used it when unavailable. Keep comments focused on non-obvious constraints and intent.

## Repository-dependent rules

Read [toolchain-policy.md](references/toolchain-policy.md) when the repository contains Oxlint, Lefthook, changelogen/release automation, Dependabot, or a curated `.agents/skills/` directory.

For Nuxt or Vue code, read and apply [nuxt-vue-conventions.md](references/nuxt-vue-conventions.md).

## Agent operations

- Do not add `Co-authored-by` or other agent attribution trailers.
- Never commit or push. Leave changes in the working tree for the user unless a later explicit instruction overrides this rule.
- Fix hook and validation failures; do not bypass them with `--no-verify`.
- Consult relevant skills under `.agents/skills/` before working in their domains. Do not load unrelated skills.

Run validation proportional to the change and report commands that could not run or failures that predate the change.
