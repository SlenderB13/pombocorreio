import fs from "node:fs";

const version = process.argv[2];

if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("Usage: npm run release:version -- 1.2.3");
  process.exit(1);
}

function readJson(path) {
  return JSON.parse(fs.readFileSync(path, "utf8"));
}

function writeJson(path, value) {
  fs.writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

const packageJson = readJson("package.json");
packageJson.version = version;
writeJson("package.json", packageJson);

const packageLock = readJson("package-lock.json");
packageLock.version = version;
packageLock.packages[""].version = version;
writeJson("package-lock.json", packageLock);

const tauriConfig = readJson("src-tauri/tauri.conf.json");
tauriConfig.version = version;
writeJson("src-tauri/tauri.conf.json", tauriConfig);

const cargoPath = "src-tauri/Cargo.toml";
const cargo = fs.readFileSync(cargoPath, "utf8");
fs.writeFileSync(cargoPath, cargo.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`));

const cargoLockPath = "src-tauri/Cargo.lock";
const cargoLock = fs.readFileSync(cargoLockPath, "utf8");
fs.writeFileSync(
  cargoLockPath,
  cargoLock.replace(/(name = "pombocorreio"\nversion = ")[^"]+(")/, `$1${version}$2`),
);

const pkgbuildPath = "packaging/arch/PKGBUILD";
const pkgbuild = fs.readFileSync(pkgbuildPath, "utf8");
fs.writeFileSync(
  pkgbuildPath,
  pkgbuild.replace(/^pkgver=.*$/m, `pkgver=${version}`).replace(/^pkgrel=.*$/m, "pkgrel=1"),
);

console.log(`Updated Pombo Correio to version ${version}`);
