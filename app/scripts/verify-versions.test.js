import test from 'node:test';
import assert from 'node:assert/strict';
import { verifyVersions } from './verify-versions.js';

test('verifyVersions returns consistent versions across all project files', () => {
  const result = verifyVersions();
  assert.ok(result.target);
  assert.equal(result.versions.packageJson, result.target);
  assert.equal(result.versions.packageLock, result.target);
  assert.equal(result.versions.cargoToml, result.target);
  assert.equal(result.versions.cargoLock, result.target);
  assert.equal(result.versions.tauriConf, result.target);
});
