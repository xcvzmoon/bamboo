# Verification

Back to [[plans/01-local-browser-jobs/overview]]

- Run `pnpm install` before the first typecheck.
- Required completion order: `pnpm check` → `pnpm typecheck` → `pnpm test` → `pnpm build`.
- If DMG packaging fails in Finder window-layout AppleScript, rerun `CI=true TAURI_BUNDLER_DMG_IGNORE_CI=false pnpm build`. Tauri's supported CI path skips Finder layout and still builds the app and DMG; no application configuration change is needed.
- Also run focused lint and behavioral tests appropriate to changed modules; wire TypeScript runner tests into the test script when introduced.
- Use temporary databases/directories, real child fixtures and local HTTP auth fixtures. Do not mock modules.
- Wikipedia is a live acceptance example, not a deterministic CI dependency.
- First milestone: register the local Wikipedia project, provide a topic input, run Lightpanda/Playwright, view logs/results, restart and inspect history, export valid JSON and flat CSV.
- Later acceptance: saved Helium login survives a Lightpanda run; cancellation preserves saved state; expired authentication requests refresh; busy/sleep/quit skip schedules.
- Check the packaged macOS app launched from Finder: executable discovery, resource loading, dependency resolution, paths with spaces, writable app-data location and exit cleanup.
- Manually inspect UI states for empty jobs, queue, running, success, failure, missing runtimes and profile refresh. Do not use preview tools unless browser work is requested.
- No commit or push is authorized.
