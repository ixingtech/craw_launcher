import fs from "node:fs";
import path from "node:path";

const bundleDir = path.resolve("src-tauri", "target", "release", "bundle", "nsis");
const sourceName = "小龙虾启动器_0.1.2_x64-setup.exe";
const targetName = "小龙虾启动器_0.1.2_x64安装包.exe";

const sourcePath = path.join(bundleDir, sourceName);
const targetPath = path.join(bundleDir, targetName);

if (!fs.existsSync(sourcePath)) {
  throw new Error(`missing NSIS artifact: ${sourcePath}`);
}

if (fs.existsSync(targetPath)) {
  fs.rmSync(targetPath, { force: true });
}

fs.renameSync(sourcePath, targetPath);
console.log(`renamed installer to ${targetName}`);
