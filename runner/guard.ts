import { spawn } from 'node:child_process';

const [executable, ...arguments_] = process.argv.slice(2);
if (!executable) {
  throw new Error('RUN_PROCESS: Missing executable');
}

function terminateGroup(): void {
  process.kill(-process.pid, 'SIGKILL');
}

process.stdin.on('end', terminateGroup);
process.stdin.on('error', terminateGroup);
process.stdin.resume();
process.on('SIGTERM', () => undefined);

const child = spawn(executable, arguments_, { stdio: ['ignore', 'inherit', 'inherit'] });
child.on('error', (error) => {
  console.error('RUN_PROCESS:', error.message);
  process.exit(1);
});
child.on('exit', (code) => {
  process.exit(code ?? 1);
});
