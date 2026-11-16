// 把版本号统一到各个清单文件。版本号以 package.json 为准（可用参数覆盖）。
// 用法：node tools/scripts/set-version.mjs            # 读 package.json
//       node tools/scripts/set-version.mjs 0.2.0      # 显式指定
import { readFileSync, writeFileSync } from "node:fs";

const root = process.cwd();
const pkgPath = `${root}/package.json`;
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
const version = (process.argv[2] ?? pkg.version ?? "").trim();

if (!/^\d+\.\d+\.\d+([-+][0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`版本号不合法: "${version}"`);
  process.exit(1);
}

function patchJson(rel, mutate) {
  const path = `${root}/${rel}`;
  let data;
  try {
    data = JSON.parse(readFileSync(path, "utf8"));
  } catch (e) {
    console.error(`跳过 ${rel}: ${e.message}`);
    return;
  }
  mutate(data);
  writeFileSync(path, JSON.stringify(data, null, 2) + "\n");
  console.log(`  ${rel} -> ${version}`);
}

patchJson("package.json", (d) => (d.version = version));
patchJson("ui/package.json", (d) => (d.version = version));
patchJson("src-tauri/tauri.conf.json", (d) => (d.version = version));

// Cargo.toml：只改 [package] 段里第一个 version
const cargoPath = `${root}/src-tauri/Cargo.toml`;
let cargo = readFileSync(cargoPath, "utf8");
if (!/^version = ".*"$/m.test(cargo)) {
  console.error("  src-tauri/Cargo.toml 里没找到 version");
  process.exit(1);
}
cargo = cargo.replace(/^version = ".*"$/m, `version = "${version}"`);
writeFileSync(cargoPath, cargo);
console.log(`  src-tauri/Cargo.toml -> ${version}`);

console.log(`version = ${version}`);
