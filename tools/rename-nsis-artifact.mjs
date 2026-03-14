import fs from "node:fs";
import path from "node:path";

const locale = process.argv[2];

if (!locale) {
  throw new Error("missing locale argument");
}

const bundleDir = path.resolve("src-tauri", "target", "release", "bundle", "nsis");
const releaseDir = path.resolve("src-tauri", "target", "release");
const VERSION = "0.1.8";

const artifactConfig = {
  "zh-CN": {
    sourceName: `xiaolongxia_${VERSION}_x64-setup.exe`,
    targetName: `xiaolongxia_${VERSION}_windows_x64.exe`,
    legacyNames: [
      `openclaw-launcher_${VERSION}_windows_x64_zh-CN.exe`,
      `xiaolongxia_${VERSION}_installer.exe`,
      `小龙虾启动器_${VERSION}_x64安装包.exe`,
      `小龙虾启动器_${VERSION}_x64-setup.exe`,
      `小龙虾启动器_${VERSION}_windows_x64.exe`
    ]
  },
  "en-US": {
    sourceName: `Craw Launcher_${VERSION}_x64-setup.exe`,
    targetName: `craw-launcher_${VERSION}_windows_x64.exe`,
    legacyNames: [`openclaw-launcher_${VERSION}_windows_x64_en-US.exe`]
  }
};

const config = artifactConfig[locale];

if (!config) {
  throw new Error(`unsupported locale: ${locale}`);
}

const sourcePath = path.join(bundleDir, config.sourceName);
const bundleTargetPath = path.join(bundleDir, config.targetName);
const releaseTargetPath = path.join(releaseDir, config.targetName);
const sourceSigPath = `${sourcePath}.sig`;
const bundleSigTargetPath = `${bundleTargetPath}.sig`;
const releaseSigTargetPath = `${releaseTargetPath}.sig`;

if (!fs.existsSync(sourcePath)) {
  throw new Error(`missing NSIS artifact: ${sourcePath}`);
}

for (const targetPath of [bundleTargetPath, releaseTargetPath, bundleSigTargetPath, releaseSigTargetPath]) {
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
    const legacySigPath = `${legacyPath}.sig`;
    if (legacySigPath !== sourceSigPath && fs.existsSync(legacySigPath)) {
      fs.rmSync(legacySigPath, { force: true });
    }
  }
}

fs.renameSync(sourcePath, bundleTargetPath);
fs.copyFileSync(bundleTargetPath, releaseTargetPath);
if (fs.existsSync(sourceSigPath)) {
  fs.renameSync(sourceSigPath, bundleSigTargetPath);
  fs.copyFileSync(bundleSigTargetPath, releaseSigTargetPath);
}

console.log(`finalized installer ${config.targetName}`);
