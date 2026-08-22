import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const appRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(appRoot, '..');

export function bumpVersion(targetVersion) {
  if (!targetVersion || !/^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$/.test(targetVersion)) {
    throw new Error(`Invalid semver version format: "${targetVersion}". Expected format like "1.2.1" or "1.2.1-beta.1"`);
  }

  const results = [];

  // 1. app/package.json
  const pkgPath = path.join(appRoot, 'package.json');
  const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
  const oldVersion = pkg.version;
  pkg.version = targetVersion;
  fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n', 'utf8');
  results.push(`app/package.json: ${oldVersion} -> ${targetVersion}`);

  // 2. app/package-lock.json
  const lockPath = path.join(appRoot, 'package-lock.json');
  if (fs.existsSync(lockPath)) {
    const lock = JSON.parse(fs.readFileSync(lockPath, 'utf8'));
    lock.version = targetVersion;
    if (lock.packages && lock.packages['']) {
      lock.packages[''].version = targetVersion;
    }
    fs.writeFileSync(lockPath, JSON.stringify(lock, null, 2) + '\n', 'utf8');
    results.push(`app/package-lock.json: synchronized to ${targetVersion}`);
  }

  // 3. app/src-tauri/Cargo.toml
  const cargoTomlPath = path.join(appRoot, 'src-tauri', 'Cargo.toml');
  let cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');
  cargoToml = cargoToml.replace(
    /(\[package\][\s\S]*?version\s*=\s*)"[^"]+"/,
    `$1"${targetVersion}"`
  );
  fs.writeFileSync(cargoTomlPath, cargoToml, 'utf8');
  results.push(`app/src-tauri/Cargo.toml: synchronized to ${targetVersion}`);

  // 4. app/src-tauri/Cargo.lock
  const cargoLockPath = path.join(appRoot, 'src-tauri', 'Cargo.lock');
  if (fs.existsSync(cargoLockPath)) {
    let cargoLock = fs.readFileSync(cargoLockPath, 'utf8');
    cargoLock = cargoLock.replace(
      /(\[\[package\]\]\r?\nname\s*=\s*"telestash"\r?\nversion\s*=\s*)"[^"]+"/,
      `$1"${targetVersion}"`
    );
    fs.writeFileSync(cargoLockPath, cargoLock, 'utf8');
    results.push(`app/src-tauri/Cargo.lock: synchronized to ${targetVersion}`);
  }

  // 5. app/src-tauri/tauri.conf.json
  const tauriConfPath = path.join(appRoot, 'src-tauri', 'tauri.conf.json');
  const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
  tauriConf.version = targetVersion;
  fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n', 'utf8');
  results.push(`app/src-tauri/tauri.conf.json: synchronized to ${targetVersion}`);

  // 6. CHANGELOG.md verification/reminder
  const changelogPath = path.join(repoRoot, 'CHANGELOG.md');
  if (fs.existsSync(changelogPath)) {
    const changelog = fs.readFileSync(changelogPath, 'utf8');
    const headerRegex = new RegExp(`##\\s*\\[?${targetVersion.replace(/\./g, '\\.')}\\]?`, 'i');
    if (!headerRegex.test(changelog)) {
      results.push(`CHANGELOG.md: WARNING - Entry for ## [${targetVersion}] not found yet! Please add release notes.`);
    } else {
      results.push(`CHANGELOG.md: Confirmed entry for [${targetVersion}] exists`);
    }
  }

  return { targetVersion, results };
}

// CLI execution
if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(__filename)) {
  const versionArg = process.argv[2];
  if (!versionArg) {
    console.error('Usage: node scripts/bump-version.js <new_version>');
    console.error('Example: node scripts/bump-version.js 1.2.1');
    process.exit(1);
  }

  try {
    const { targetVersion, results } = bumpVersion(versionArg);
    console.log(`\nSuccessfully bumped version to ${targetVersion}:`);
    for (const res of results) {
      console.log(`  - ${res}`);
    }
    console.log('\nAll configuration and lock files are now 100% in sync for release!\n');
  } catch (err) {
    console.error(`Error bumping version: ${err.message}`);
    process.exit(1);
  }
}
