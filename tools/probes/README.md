# Lightpanda runtime probe

Run `pnpm probe:runtime` to start installed Lightpanda on loopback, connect through
Playwright, and extract a local page whose heading is set by JavaScript. The JSON
report is emitted after the browser context and process have been closed.

Run `pnpm probe:runtime https://en.wikipedia.org/wiki/Bamboo` for the live acceptance
example. Public HTTPS URLs are accepted as the optional argument. Live sites are
not part of the default test suite.

Set `LIGHTPANDA_EXECUTABLE` to select an executable, including paths containing
spaces. The default is `lightpanda` from PATH. The probe does not download browsers
or install runtimes. Lightpanda telemetry is disabled for its child process.

`pnpm test` includes deterministic process failure tests. Set
`BAMBOO_RUNTIME_TEST=1 pnpm test:unit tests/probes` to include the installed Lightpanda test.
Startup waits at most 10 seconds, CDP connection 5 seconds, and navigation 15
seconds. A 35-second overall deadline also terminates the child during stalled
CDP operations. Shutdown sends SIGTERM and escalates to SIGKILL after 2 seconds. SIGINT
and SIGTERM trigger cleanup. SIGKILL of the probe cannot run cleanup handlers.
The ephemeral CDP port is reserved then released before Lightpanda binds it;
a competing bind produces a startup failure rather than an automatic retry.

Failures include `PROBE_EXECUTABLE`, `PROBE_STARTUP`, `PROBE_STARTUP_TIMEOUT`,
`PROBE_NAVIGATION`, and `PROBE_EXTRACTION`. Validation and Playwright errors retain
their original diagnostics. Lightpanda startup diagnostics retain at most 4 KiB.

## Verified on 2026-10-26

- macOS Apple Silicon (`darwin`, `arm64`)
- Node `v26.8.1` with native TypeScript execution
- Lightpanda `1.1.0-nightly.10071+3be033135`
- playwright-core `1.64.1` (exact dependency pin)
- Local JavaScript extraction and Wikipedia title/heading extraction passed.
- Both runs confirmed context closure and process termination.

The installed nightly is a tested version, not a stable release guarantee. Intel
Mac binaries exist in the upstream 1.0.2 release for both aarch64 and x86_64
([release assets](https://github.com/lightpanda-io/browser/releases/tag/1.0.2)),
but Intel execution and packaged application runtime discovery have not been verified. This
probe establishes the external runtime connection; it does not yet add job
registration, persistence, or an application runner.
