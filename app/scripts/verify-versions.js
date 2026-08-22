import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const appRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(appRoot, '..');

export function verifyVersions(expectedVersion) {
  const errors = [];
  const versions = {};

  // 1. app/package.json
  const pkgPath = path.join(appRoot, 'package.json');
  const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
  versions.packageJson = pkg.version;

  const target = expectedVersion || pkg.version;

  if (pkg.version !== target) {
    errors.push(`app/package.json version "${pkg.version}" does not match target "${target}"`);
  }

  // 2. app/package-lock.json
  const lockPath = path.join(appRoot, 'package-lock.json');
  if (fs.existsSync(lockPath)) {
    const lock = JSON.parse(fs.readFileSync(lockPath, 'utf8'));
    versions.packageLock = lock.version;
    if (lock.version !== target) {
      errors.push(`app/package-lock.json version "${lock.version}" does not match target "${target}"`);
    }
    if (lock.packages && lock.packages[''] && lock.packages[''].version !== target) {
      errors.push(`app/package-lock.json packages[""].version "${lock.packages[''].version}" does not match target "${target}"`);
    }
  }

  // 3. app/src-tauri/Cargo.toml
  const cargoTomlPath = path.join(appRoot, 'src-tauri', 'Cargo.toml');
  const cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');
  const cargoMatch = cargoToml.match(/\[package\][\s\S]*?version\s*=\s*"([^"]+)"/);
  if (!cargoMatch) {
    errors.push(`Could not find package version in app/src-tauri/Cargo.toml`);
  } else {
    versions.cargoToml = cargoMatch[1];
    if (cargoMatch[1] !== target) {
      errors.push(`app/src-tauri/Cargo.toml version "${cargoMatch[1]}" does not match target "${target}"`);
    }
  }

  // 4. app/src-tauri/Cargo.lock
  const cargoLockPath = path.join(appRoot, 'src-tauri', 'Cargo.lock');
  if (fs.existsSync(cargoLockPath)) {
    const cargoLock = fs.readFileSync(cargoLockPath, 'utf8');
    const lockMatch = cargoLock.match(/\[\[package\]\]\r?\nname\s*=\s*"telestash"\r?\nversion\s*=\s*"([^"]+)"/);
    if (!lockMatch) {
      errors.push(`Could not find telestash package version in app/src-tauri/Cargo.lock`);
    } else {
      versions.cargoLock = lockMatch[1];
      if (lockMatch[1] !== target) {
        errors.push(`app/src-tauri/Cargo.lock telestash version "${lockMatch[1]}" does not match target "${target}" (will cause cargo check --locked failure in CI)`);
      }
    }
  }

  // 5. app/src-tauri/tauri.conf.json
  const tauriConfPath = path.join(appRoot, 'src-tauri', 'tauri.conf.json');
  const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
  versions.tauriConf = tauriConf.version;
  if (tauriConf.version !== target) {
    errors.push(`app/src-tauri/tauri.conf.json version "${tauriConf.version}" does not match target "${target}"`);
  }

  // 6. CHANGELOG.md
  const changelogPath = path.join(repoRoot, 'CHANGELOG.md');
  if (fs.existsSync(changelogPath)) {
    const changelog = fs.readFileSync(changelogPath, 'utf8');
    const headerRegex = new RegExp(`##\\s*\\[?${target.replace(/\./g, '\\.')}\\]?`, 'i');
    if (!headerRegex.test(changelog)) {
      errors.push(`CHANGELOG.md does not contain an entry for "## [${target}]"`);
    }
  }

  if (errors.length > 0) {
    throw new Error(`Version synchronization errors detected:\n${errors.map(e => `  - ${e}`).join('\n')}\n\nRun 'npm run bump-version ${target}' to synchronize all files automatically.`);
  }

  return { target, versions };
}

// CLI execution
if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(__filename)) {
  const targetVersion = process.argv[2];
  try {
    const { target, versions } = verifyVersions(targetVersion);
    console.log(`[Version Verification] All 6 configuration, lock, and changelog files are 100% in sync at version ${target}:`);
    for (const [k, v] of Object.entries(versions)) {
      console.log(`  - ${k}: ${v}`);
    }
    process.exit(0);
  } catch (err) {
    console.error(`\n[Version Verification FAILED]`);
    console.error(err.message);
    process.exit(1);
  }
}
