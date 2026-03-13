import fs from "node:fs";
import path from "node:path";

const [locale, version, tag, repo, pubDate] = process.argv.slice(2);

if (!locale || !version || !tag || !repo || !pubDate) {
  throw new Error("usage: node tools/generate-updater-manifest.mjs <locale> <version> <tag> <repo> <pubDate>");
}

const releaseDir = path.resolve("src-tauri", "target", "release");
const bundleDir = path.resolve("src-tauri", "target", "release", "bundle", "nsis");
const assetName =
  locale === "zh-CN"
    ? `xiaolongxia_${version}_windows_x64.exe`
    : `craw-launcher_${version}_windows_x64.exe`;
const sigName = `${assetName}.sig`;
const manifestName = locale === "zh-CN" ? "latest-zh-CN.json" : "latest-en-US.json";
const notes =
  locale === "zh-CN"
    ? `xiaolongxia ${version} is now available.`
    : `Craw Launcher ${version} is now available.`;

const sigPath = path.join(releaseDir, sigName);
if (!fs.existsSync(sigPath)) {
  const bundleSigPath = path.join(bundleDir, sigName);
  throw new Error(
    `missing signature file: ${sigPath} (also checked ${bundleSigPath}); ensure bundle.createUpdaterArtifacts is enabled`
  );
}

const manifest = {
  version,
  notes,
  pub_date: pubDate,
  platforms: {
    "windows-x86_64": {
      signature: fs.readFileSync(sigPath, "utf8").trim(),
      url: `https://github.com/${repo}/releases/download/${tag}/${assetName}`
    }
  }
};

fs.writeFileSync(path.join(releaseDir, manifestName), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`generated ${manifestName}`);
