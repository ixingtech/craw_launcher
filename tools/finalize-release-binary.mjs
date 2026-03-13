import fs from "node:fs";
import path from "node:path";

const locale = process.argv[2];

if (!locale) {
  throw new Error("missing locale argument");
}

const releaseDir = path.resolve("src-tauri", "target", "release");
const sourceName = "openclaw_launcher.exe";
const sourcePath = path.join(releaseDir, sourceName);

const targetNameByLocale = {
  "zh-CN": "小龙虾启动器.exe",
  "en-US": "Craw Launcher.exe"
};

const targetName = targetNameByLocale[locale];

if (!targetName) {
  throw new Error(`unsupported locale: ${locale}`);
}

if (!fs.existsSync(sourcePath)) {
  throw new Error(`missing release binary: ${sourcePath}`);
}

const targetPath = path.join(releaseDir, targetName);
if (fs.existsSync(targetPath)) {
  fs.rmSync(targetPath, { force: true });
}
fs.copyFileSync(sourcePath, targetPath);

const sourcePdb = path.join(releaseDir, "openclaw_launcher.pdb");
if (fs.existsSync(sourcePdb)) {
  const targetPdb = path.join(releaseDir, targetName.replace(/\.exe$/i, ".pdb"));
  if (fs.existsSync(targetPdb)) {
    fs.rmSync(targetPdb, { force: true });
  }
  fs.copyFileSync(sourcePdb, targetPdb);
}

console.log(`finalized release binary ${targetName}`);
