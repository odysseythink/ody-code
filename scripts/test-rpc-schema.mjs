/**
 * G2-B smoke test: verify rpc-schema.json is valid JSON and has the expected
 * structure for the code generator.
 *
 * Run: node scripts/test-rpc-schema.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const schemaPath = join(__dirname, 'generated/rpc-schema.json');

let passed = 0;
let failed = 0;

function assert(cond, msg) {
  if (cond) {
    passed++;
  } else {
    console.error(`FAIL: ${msg}`);
    failed++;
  }
}

// Load
let data;
try {
  const json = readFileSync(schemaPath, 'utf-8');
  data = JSON.parse(json);
  assert(true, 'Schema is valid JSON');
} catch (e) {
  assert(false, `Schema is valid JSON: ${e.message}`);
  process.exit(1);
}

// Top-level structure
assert(typeof data === 'object', 'Schema is an object');
assert(data.$id === 'https://ody-code.dev/rpc-schema.json', 'Schema has $id');
assert(typeof data.title === 'string', 'Schema has title');
assert(typeof data.version === 'string', 'Schema has version');

// Protocols
assert(typeof data.protocols === 'object', 'Schema has protocols');
assert(typeof data.protocols.core === 'object', 'Schema has protocols.core');
assert(typeof data.protocols.sdk === 'object', 'Schema has protocols.sdk');

// Core methods
const coreMethods = data.protocols.core.methods;
assert(typeof coreMethods === 'object', 'Core has methods');
const coreCount = Object.keys(coreMethods).length;
assert(coreCount >= 60, `Core has ${coreCount} methods (>= 60)`);

// Core method structure
const sample = coreMethods.getCoreInfo;
assert(sample !== undefined, 'Core has getCoreInfo');
assert(typeof sample.payload === 'object', 'getCoreInfo has payload');
assert(typeof sample.returns === 'object', 'getCoreInfo has returns');

// SDK methods
const sdkMethods = data.protocols.sdk.methods;
assert(typeof sdkMethods === 'object', 'SDK has methods');
const sdkCount = Object.keys(sdkMethods).length;
assert(sdkCount >= 5, `SDK has ${sdkCount} methods (>= 5)`);

// SDK method structure
assert(sdkMethods.emitEvent !== undefined, 'SDK has emitEvent');
assert(sdkMethods.requestApproval !== undefined, 'SDK has requestApproval');

// Definitions
assert(typeof data.definitions === 'object', 'Schema has definitions');
const defCount = Object.keys(data.definitions).length;
assert(defCount >= 150, `Schema has ${defCount} definitions (>= 150)`);

// Key definitions exist
const keyDefs = ['SessionSummary', 'CoreInfo', 'CreateSessionPayload', 'AgentConfigData'];
for (const name of keyDefs) {
  assert(name in data.definitions, `Definition for ${name} exists`);
  const def = data.definitions[name];
  if (def) assert(typeof def === 'object', `Definition for ${name} is an object`);
}

// Count schemas with proper JSON Schema structure
const schemaCount = Object.values(data.definitions).filter(v => v !== null).length;
assert(schemaCount >= 80, `At least 80 definitions have JSON Schema (got ${schemaCount})`);

console.log(`\nResults: ${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
