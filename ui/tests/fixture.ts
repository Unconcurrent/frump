import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

export const boardPath = join(tmpdir(), 'frump-ui-browser', 'frump.md');
export function resetBoard(large = false) {
  const log = large ? 'Large document evidence. '.repeat(90_000) : 'Small document evidence.';
  writeFileSync(boardPath, `# Mission control\n\n## Team\n\n* Ada <ada@example.com> - Engineer\n\n## Tasks\n\n### Task 1 - Design the navigation\n\n${log}\n\nStatus: open\nAssigned To: Ada\nPriority: high\n\n### Bug 2 - Repair the task panel\n\nSecond task body.\n\nStatus: working\n\n### Task 3 - Validate the release\n\nDone and ready to close.\n\nStatus: done\n`);
}
