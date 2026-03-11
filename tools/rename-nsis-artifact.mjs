import fs from "node:fs";
import path from "node:path";

const locale = process.argv[2];

if (!locale) {
  throw new Error("missing locale argument");
}

const bundleDir = path.resolve("src-tauri", "target", "release", "bundle", "nsis");
const releaseDir = path.resolve("src-tauri", "target", "release");

const artifactConfig = {
  "zh-CN": {
    sourceName: "小龙虾启动器_0.1.2_x64-setup.exe",
    targetName: "小龙虾启动器_0.1.2_windows_x64.exe",
    legacyNames: [
      "openclaw-launcher_0.1.2_windows_x64_zh-CN.exe",
      "小龙虾启动器_0.1.2_x64安装包.exe",
    ],
  },
  "en-US": {
    sourceName: "Craw Launcher_0.1.2_x64-setup.exe",
    targetName: "craw-launcher_0.1.2_windows_x64.exe",
    legacyNames: ["openclaw-launcher_0.1.2_windows_x64_en-US.exe"],
  },
};

const config = artifactConfig[locale];

if (!config) {
  throw new Error(`unsupported locale: ${locale}`);
}

const sourcePath = path.join(bundleDir, config.sourceName);
const bundleTargetPath = path.join(bundleDir, config.targetName);
const releaseTargetPath = path.join(releaseDir, config.targetName);

if (!fs.existsSync(sourcePath)) {
  throw new Error(`missing NSIS artifact: ${sourcePath}`);
}

for (const targetPath of [bundleTargetPath, releaseTargetPath]) {
  if (fs.existsSync(targetPath)) {
    fs.rmSync(targetPath, { force: true });
  }
}

for (const legacyName of config.legacyNames) {
  for (const dir of [bundleDir, releaseDir]) {
    const legacyPath = path.join(dir, legacyName);
    if (legacyPath !== sourcePath && fs.existsSync(legacyPath)) {
      fs.rmSync(legacyPath, { force: true });
    }
  }
}

fs.renameSync(sourcePath, bundleTargetPath);
fs.copyFileSync(bundleTargetPath, releaseTargetPath);

console.log(`finalized installer ${config.targetName}`);
