#!/usr/bin/env node
/**
 * Micro-benchmark: @odysseythink/ody-crypto native binary vs TS fallback.
 *
 * Run from repository root:
 *   node .ody-code/poc/native-vs-fallback-bench.mjs
 *
 * Prints median latency (ns/op) and throughput (ops/s) for each function.
 */

import { createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes as nodeRandomBytes } from 'node:crypto';
import jwt from 'jsonwebtoken';
const { sign } = jwt;
import { getOdyCrypto, loadNative, tsFallback } from '@odysseythink/ody-crypto';

const native = loadNative();
const fallback = tsFallback;

if (!native) {
  console.error('Native binary not available for this target; benchmark cannot compare.');
  process.exit(1);
}

// Prepare JWT / JWK for verifyIdToken benchmark.
const { privateKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
const jwk = createPublicKey(privateKey).export({ format: 'jwk' });
const jwkJson = JSON.stringify(jwk);
const idToken = sign({ sub: 'user-1', aud: 'client-1', iss: 'issuer.example.test' }, privateKey, {
  algorithm: 'RS256',
  expiresIn: '1h',
});
const expected = { issuer: 'issuer.example.test', audience: 'client-1', maxAgeSeconds: 3600 };

const shaInput = nodeRandomBytes(1024).toString('hex');

function bench(name, fn, warmupMs = 500, measureMs = 2000) {
  // Warmup
  const warmupEnd = performance.now() + warmupMs;
  while (performance.now() < warmupEnd) fn();

  // Measure
  const runs = [];
  const start = performance.now();
  let count = 0;
  while (performance.now() - start < measureMs) {
    const t0 = performance.now();
    fn();
    const t1 = performance.now();
    runs.push((t1 - t0) * 1e6); // ns
    count++;
  }
  runs.sort((a, b) => a - b);
  const median = runs[Math.floor(runs.length / 2)];
  const totalNs = runs.reduce((a, b) => a + b, 0);
  const avg = totalNs / runs.length;
  const throughput = (runs.length / (totalNs / 1e9)).toFixed(0);
  console.log(`${name}: count=${count.toLocaleString()} median=${Math.round(median).toLocaleString()} ns/op avg=${Math.round(avg).toLocaleString()} ns/op throughput=${throughput} ops/s`);
}

function runSuite(implName, impl) {
  console.log(`\n=== ${implName} ===`);
  bench('randomBytes(32)', () => impl.randomBytes(32));
  bench('sha256(1 KB hex)', () => impl.sha256(shaInput));
  bench('pkceChallenge()', () => impl.pkceChallenge());
  bench('verifyIdToken(RS256)', () => impl.verifyIdToken(idToken, jwkJson, expected));
}

runSuite('native', native);
runSuite('tsFallback', fallback);

// End-to-end mixed workload: simulate 1000 OAuth authorizations (PKCE + random state + id_token verify).
function mixedWorkload(impl) {
  for (let i = 0; i < 1000; i++) {
    impl.randomBytes(32);
    impl.pkceChallenge();
    impl.verifyIdToken(idToken, jwkJson, expected);
  }
}

console.log('\n=== mixed workload (1000 OAuth flows) ===');
for (const [name, impl] of [['native', native], ['tsFallback', fallback]]) {
  const t0 = performance.now();
  mixedWorkload(impl);
  const t1 = performance.now();
  console.log(`${name}: ${(t1 - t0).toFixed(2)} ms`);
}
