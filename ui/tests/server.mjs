import { mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn, execFileSync } from 'node:child_process';

const root = join(tmpdir(), 'frump-ui-browser');
mkdirSync(root, { recursive: true });
writeFileSync(join(root, 'frump.md'), '# Browser checks\n\n## Tasks\n');
execFileSync('git', ['init', '-q', root]);
execFileSync('git', ['-C', root, 'config', 'user.name', 'Browser tests']);
execFileSync('git', ['-C', root, 'config', 'user.email', 'tests@example.com']);
if (!process.env.FRUMP_TEST_BINARY) execFileSync('cargo', ['build', '--quiet'], { cwd: '..', stdio: 'inherit' });
const env = { ...process.env, PATH: '/usr/bin:/bin' };
delete env.METATEAM_CREW_AGENT;
const child = spawn(process.env.FRUMP_TEST_BINARY ?? '../target/debug/frump', ['web', '--board', join(root, 'frump.md'), '--port', '4317'], { stdio: 'inherit', env });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
child.on('exit', code => process.exit(code ?? 0));
