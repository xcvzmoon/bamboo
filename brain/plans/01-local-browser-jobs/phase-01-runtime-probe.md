# 1. Prove the external runtime connection

Back to [[plans/01-local-browser-jobs/overview]]

## Goal

Prove the external runtime connection. Follow [[principles/grow-in-working-layers]] and [[principles/keep-changes-focused]].

## Changes

- `tools/probes/lightpanda.ts`
- `package.json`
- `pnpm-lock.yaml`

Use installed Node.js and Lightpanda with playwright-core. Prove navigation, extraction, context cleanup, and process termination against pinned tested versions. Check Apple Silicon versus Intel availability before declaring supported Mac architectures. Use a loopback endpoint and dynamically allocated port. No browser downloads.

## Data structures

- RuntimeProbeResult: versions, endpoint readiness, supported operations.

## Routing

- Implementation in the active coding session; no provider or model requirement.
- Listed paths describe primary implementation boundaries; keep required dependency manifests, lockfiles and focused tests with the behavior they support.

## Verification

- Static: follow the ordered repository checks in [[plans/01-local-browser-jobs/testing]].
- Runtime: Run Wikipedia extraction and a deterministic local fixture. Missing executable and startup failure produce stable errors; no child process survives probe exit.
- Add focused behavioral tests for state transitions and failure boundaries; no module mocking.
