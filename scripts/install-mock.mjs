// 把模拟插件打包成单文件并安装到 %APPDATA%\desktop-monitor\plugins\mock
import { build } from "esbuild";
import { copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";

const appData = process.env.APPDATA;
if (!appData) {
  throw new Error("APPDATA 未设置，无法定位插件目录");
}

const outDir = path.join(appData, "desktop-monitor", "plugins", "mock");
mkdirSync(outDir, { recursive: true });

await build({
  entryPoints: ["plugins/mock/src/index.ts"],
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node20",
  outfile: path.join(outDir, "index.cjs"),
  logLevel: "info",
});

copyFileSync("plugins/mock/manifest.json", path.join(outDir, "manifest.json"));
console.log(`mock 插件已安装到 ${outDir}`);
