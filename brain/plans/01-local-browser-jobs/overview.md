# Local browser jobs

## Context

Bamboo becomes a macOS desktop tool for its developer owner to run trusted local TypeScript browser jobs on demand or while the app is running. This records the product interview decisions of 2026-10-26; implementation has not started.

## Scope

- Nuxt/Tauri interface; Rust owns Lightpanda, scheduling, queue, persistence and cleanup.
- Separate Node process per job using Playwright over Lightpanda CDP; no custom Rust automation API.
- User installs Node, Lightpanda and project dependencies. Installed Chrome is used only for manual login capture; no bundled/downloaded Chromium or execution fallback.
- Register local project/entry; named JSON inputs saved per job, editable for manual runs; no input presets initially.
- Trusted scripts may use npm, filesystem, network and project-managed environment configuration. No sandbox or Bamboo secret manager.
- Return hundreds to thousands of JSON records; separate dataset per successful run; flat CSV export.
- One active run globally, manual FIFO queue, configurable timeout/cancellation, logs, no retries.
- Named saved login profiles; manual refresh; explicit script signal for expired authentication.
- Intervals and daily local schedules run only while the app is open; skip busy and missed occurrences. Closing the window quits.
- Keep history until deleted; record entry-file hash, not full source/dependency snapshots.
- Exclude AI, visual workflow authoring, cloud execution, cron, automatic dependency installation, session universality, million-record streaming and Windows/Linux.

## Design decisions

- Follow [[principles/grow-in-working-layers]]: deliver Wikipedia run-to-export before profiles or scheduling.
- Follow [[principles/keep-changes-focused]]: retain the starter shell and add small Rust modules and Vue panels.
- Alternatives: Rust CDP automation would duplicate the TypeScript authoring path; standalone arbitrary scripts would weaken the result/lifecycle contract. Choose a managed job function with Playwright context and JSON input.
- SQLite metadata plus app-data artifacts gives durable queryable history without an external service. A JSON-only store would complicate concurrent state updates; a remote database is unnecessary.
- Job code/dependencies execute from the user's project; distribute the runner and its private dependencies as app resources, with an established TypeScript loader. Verify packaged resolution before selecting exact packaging configuration.
- Rust state is authoritative; narrow Tauri commands mutate it and events notify the UI to refresh. Restrict frontend capabilities; trusted job access remains unrestricted.
- Profile transfer is a compatibility gate, not a guarantee. Explicitly test installed Chrome to Lightpanda cookie/local-storage round trips before building profile UX.
- Initial runtime/version and Mac architecture support are evidence-based outputs of phase 1. Do not claim all macOS hardware is supported before checking binaries.

## Applicable skills

- Planning: plan, brain, tauri-app-planning.
- Implementation: execute, xcvzmoon-code-conventions, tauri-v2, tauri-ipc, vue-best-practices, ts-best-practices.
- Runtime packaging: tauri-app-sidecar-nodejs, tauri-config; adapt for externally installed Node rather than bundling its executable.
- Tests and UI: testing, vitest, vue-testing-best-practices, frontend-design.

## Phases

- [[plans/01-local-browser-jobs/phase-01-runtime-probe]]
- [[plans/01-local-browser-jobs/phase-02-job-contract]]
- [[plans/01-local-browser-jobs/phase-03-runner-protocol]]
- [[plans/01-local-browser-jobs/phase-04-run-storage]]
- [[plans/01-local-browser-jobs/phase-05-run-supervisor]]
- [[plans/01-local-browser-jobs/phase-06-job-registration]]
- [[plans/01-local-browser-jobs/phase-07-results-export]]
- [[plans/01-local-browser-jobs/phase-08-cancellation-recovery]]
- [[plans/01-local-browser-jobs/phase-09-manual-queue]]
- [[plans/01-local-browser-jobs/phase-10-session-compatibility]]
- [[plans/01-local-browser-jobs/phase-11-saved-profiles]]
- [[plans/01-local-browser-jobs/phase-12-profile-runs]]
- [[plans/01-local-browser-jobs/phase-13-schedule-calculation]]
- [[plans/01-local-browser-jobs/phase-14-schedule-dispatch]]
- [[plans/01-local-browser-jobs/phase-15-schedule-history-ui]]

## Verification

See [[plans/01-local-browser-jobs/testing]]. Phases 1–7 deliver the first vertical slice; phases 8–9 harden execution; phases 10–12 add saved sessions; phases 13–15 add schedules and finish history controls.

## Sources

- [Lightpanda Playwright CDP integration](https://github.com/lightpanda-io/docs/blob/main/src/content/usage/cdp/playwright.mdx).
- [Playwright CDP connection limitations](https://playwright.dev/docs/api/class-browsertype#browser-type-connect-over-cdp).
- [Playwright authentication state](https://playwright.dev/docs/auth). State export support alone does not establish Lightpanda import compatibility.
