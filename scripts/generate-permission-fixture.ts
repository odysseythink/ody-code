import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Generate TS-side permission fixture JSON matching the Rust fixture structure.
// This allows TS tests to read the Rust-generated fixture and assert parity.
// Run: npx tsx scripts/generate-permission-fixture.ts

const scenarios = [
  { name: 'yolo-mode-approve', description: 'Yolo mode approves any tool', mode: 'yolo', toolName: 'Bash', rules: [], expectedDecision: 'approve', expectedMessageContains: null },
  { name: 'auto-mode-approve', description: 'Auto mode approves any tool', mode: 'auto', toolName: 'Bash', rules: [], expectedDecision: 'approve', expectedMessageContains: null },
  { name: 'manual-fallback-ask', description: 'Manual mode with no rules asks', mode: 'manual', toolName: 'Bash', rules: [], expectedDecision: 'ask', expectedMessageContains: null },
  { name: 'deny-rule-blocks', description: 'User deny rule blocks Write', mode: 'manual', toolName: 'Write', rules: [{ decision: 'deny', scope: 'user', pattern: 'Write' }], expectedDecision: 'deny', expectedMessageContains: 'denied by permission rule' },
  { name: 'allow-rule-approves', description: 'User allow rule approves Read', mode: 'manual', toolName: 'Read', rules: [{ decision: 'allow', scope: 'user', pattern: 'Read' }], expectedDecision: 'approve', expectedMessageContains: null },
];

const outDir = path.join(__dirname, '..', 'rust-ody', 'crates', 'agent-rs', 'tests', 'fixtures');
fs.mkdirSync(outDir, { recursive: true });
fs.writeFileSync(path.join(outDir, 'permission-scenarios-ts.json'), JSON.stringify(scenarios, null, 2));
console.log('Wrote TS permission fixture to', outDir + '/permission-scenarios-ts.json');
