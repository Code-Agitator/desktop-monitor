// 发布并安装设备电量插件（自包含单文件）
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, existsSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const proj = path.join(root, "plugins", "builtin", "device-battery");
const out = path.join(root, "build", "device-battery");

console.log("dotnet publish …");
execFileSync("dotnet", ["publish", "-c", "Release", "-o", out], {
  cwd: proj,
  stdio: "inherit",
});

const appData = process.env.APPDATA;
if (!appData) throw new Error("APPDATA 未设置");

const dest = path.join(appData, "desktop-monitor", "plugins", "device-battery");
mkdirSync(dest, { recursive: true });

const exe = path.join(out, "DeviceBattery.exe");
if (!existsSync(exe)) throw new Error("发布产物缺失：" + exe);
copyFileSync(exe, path.join(dest, "DeviceBattery.exe"));
copyFileSync(path.join(proj, "manifest.json"), path.join(dest, "manifest.json"));
console.log("已安装到", dest);
