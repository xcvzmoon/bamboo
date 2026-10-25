<!--
Use a Conventional Commit title, such as:
  feat: add a desktop shortcut
  fix: handle a failed Tauri command
  chore(ci): update action versions
-->

## Summary

<!-- Start every pull request description with one paragraph of two or three complete sentences before any heading, list, table, or diagram. Explain the problem or capability, resulting behavior, and why the change is needed. -->

- What user or system behavior changed?
- Why does this change belong in this PR?
- Is this PR independent or stacked?

## Change map

<!-- Optional: use when the diff changes several modules, responsibilities, or ownership boundaries. -->

| Module                 | Before                                  | After                              | Description           |
| ---------------------- | --------------------------------------- | ---------------------------------- | --------------------- |
| `<module or boundary>` | `<previous responsibility or behavior>` | `<new responsibility or behavior>` | `<reason and effect>` |

## Architecture and behavior

<!-- Optional: add when a diagram explains the change faster or more accurately than prose. -->

## Boundaries and risks

<!-- Optional: use when the change has meaningful failure modes, invariants, migrations, or external effects. -->

| Invariant or boundary | Failure mode                                                  | Protection                                         | Evidence or gap                               |
| --------------------- | ------------------------------------------------------------- | -------------------------------------------------- | --------------------------------------------- |
| `<required behavior>` | `<duplicate, timeout, race, invalid input, or other failure>` | `<guard, ownership rule, transaction, or cleanup>` | `<test, runtime evidence, or unverified gap>` |

## Verification

<!-- List exact test, typecheck, lint, and build commands and their results. -->

- [ ] `pnpm fmt`
- [ ] `pnpm lint`
- [ ] `pnpm check`
- [ ] `pnpm typecheck`
- [ ] `pnpm test`
- [ ] `pnpm build`
- [ ] Tests and documentation are updated where needed.

## Visual changes

<!-- Optional: use for user-visible UI changes with before/after comparison. -->
