# TypeScript jobs

A job exports a default async function. Bamboo supplies a fresh Playwright browser
context and JSON input. The function returns an array of JSON records; an empty
array is valid. Throw an Error to fail the run.

```typescript
import type { JobContext, JobResult } from '/path/to/bamboo/runner/job.ts';

export default async function job({ context, input }: JobContext): Promise<JobResult> {
  const page = await context.newPage();
  await page.goto('https://example.com');
  return [{ title: await page.title(), input }];
}
```

Bamboo closes the supplied context after success or failure. Jobs must not close
the browser or create additional contexts. Jobs own cleanup of their other
resources. These are trusted local scripts, with filesystem, network, environment,
and npm access. There is no sandbox.

## Run a job

Start your installed Lightpanda in a separate terminal:

```sh
lightpanda serve --host 127.0.0.1 --port 9222
```

From the Bamboo repository, prepare a private request directory:

```sh
node --input-type=module <<'JS'
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { createRunDirectory } from './runner/protocol.ts';

const directory = await createRunDirectory(tmpdir(), {
  version: 1,
  projectDirectory: resolve('.'),
  entryFile: 'examples/wikipedia/job.ts',
  endpoint: 'http://127.0.0.1:9222',
  input: { topic: 'Bamboo' },
});
console.info(directory);
JS
```

Pass the printed directory to `pnpm runner:job "/printed/run/directory"`.
Read its `outcome.json` after the process exits. stdout and stderr contain logs
only; they never carry the result protocol. Paths containing spaces must be
quoted. Use a fresh directory and a dedicated Lightpanda instance for each run.

The runner changes its working directory to `projectDirectory` before loading the
job. The project path must be absolute; `entryFile` resolves against it. Imports
resolve from the job's location, including its project's node_modules. Install job
dependencies in that project yourself. Environment variables are inherited; the
runner does not read .env files. A project can load its own configuration in its
job module.

The runner connects to the supplied browser, creates and closes the job context,
and disconnects. The caller owns the Lightpanda process. CDP connection waits at
most five seconds. The managed Rust supervisor adds deadlines and normal app-shutdown cleanup.
This direct CLI invocation leaves those responsibilities to its caller.

## File protocol, version 1

`request.json` contains `version`, `projectDirectory`, `entryFile`, `endpoint`, and
`input`, as shown above. The endpoint must use HTTP or WebSocket on
`127.0.0.1` with an explicit port. Unknown fields and unsupported versions fail
validation.

A successful `outcome.json` contains:

```json
{ "version": 1, "status": "success", "records": [{ "title": "Bamboo" }] }
```

A failed run publishes an error with a stable code and no records:

```json
{
  "version": 1,
  "status": "failure",
  "error": { "code": "JOB_EXECUTION", "message": "Article request failed" }
}
```

Failure codes are `REQUEST_INVALID`, `JOB_IMPORT`, `JOB_EXPORT`,
`BROWSER_CONNECT`, `JOB_EXECUTION`, `JOB_RESULT`, and `BROWSER_CLEANUP`.
Diagnostics also go to stderr, and failures set a nonzero exit status.
A request that cannot be read in a valid private directory produces a failure
outcome. An invalid directory or publication failure can leave no outcome.

Result records must contain plain JSON data. Validation rejects undefined values,
cycles, non-finite numbers, BigInts, functions, symbols, class instances, accessors,
sparse arrays, and properties JSON would silently omit. Repeated references that
do not form cycles are accepted. Ordinary object keys, including `__proto__` and
`constructor`, are preserved. Empty result arrays are valid. Values must fit
Node's JSON serialization limits.

Run directories must have mode 0700; request and outcome files use mode 0600.
Readers require current-user ownership, reject group/world access and symlinked
protocol files, and accept only regular files. `createRunDirectory` creates these
permissions. The directory identity associates the request and outcome with one
run; directories must never be reused.

Publication writes and flushes a temporary file beside the outcome, then creates
`outcome.json` with an atomic hard link. An existing outcome is never overwritten,
including when two publishers race. Readers see either no outcome or the complete
file. Temporary files are removed after normal publication or errors; an abrupt
process termination can leave a temporary file, which readers must ignore.

Consumers must wait for the process to exit and require both a zero exit status
and a validated success outcome before accepting records. `readRunOutcome`
validates the file and reports `OUTCOME_MISSING` or `OUTCOME_INVALID`; it does not
check process status. A job can call `process.exit(0)` without completing, so a
zero exit status alone is insufficient. Output resembling an outcome in logs has
no authority. The caller owns the run directory and removes it after consuming
the outcome, including failed or interrupted runs.

## Supported runtime

The runner requires Node 26 or newer. Tested on macOS Apple Silicon with Node
26.8.1, tsx 4.23.15, playwright-core 1.64.1 and Lightpanda
1.1.0-nightly.10071+3be033135.

Jobs use ESM .ts or .mts files. Set `"type": "module"` in the job project's
package.json. The scoped tsx loader supports type annotations, relative TypeScript
imports, npm imports, enums, async functions and top-level await. It does not
perform typechecking. Run your project's typechecker before running its jobs.
Bamboo disables tsconfig lookup for job loading, so tsconfig path aliases and
compiler transforms are not part of the job contract. Decorator metadata, JSX,
CommonJS entry points, and alternate runtime loaders are outside this contract.

`pnpm test:unit tests/runner` tests loading and CLI validation without an installed browser.
Run `BAMBOO_RUNTIME_TEST=1 pnpm test:unit tests/runner` to include local Lightpanda execution,
context cleanup and real child-process tests. Add `BAMBOO_WIKIPEDIA_TEST=1` to
include live Wikipedia extraction; that live site is excluded from default tests.
