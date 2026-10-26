# 2. Define and execute a TypeScript job

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Define and execute a TypeScript job. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `runner/job.ts`
- `runner/main.ts`
- `examples/wikipedia/job.ts`

Export a default async job function. Supply an isolated browser context; browser lifecycle belongs to Bamboo. Execute local ESM TypeScript using an established TypeScript loader, resolving job imports from its own project. Explicitly declare supported Node and TypeScript features. Keep project environment loading project-owned.

## Data structures

- JobContext: Playwright browser context and JSON input; JobResult: array of JSON records.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Invoke the runner against the probe browser; verify JSON inputs, local dependency resolution, and Wikipedia records. A thrown error exits unsuccessfully.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
