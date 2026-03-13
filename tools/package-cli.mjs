import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const version = pkg.version;
const targetDir = path.join(root, "src-tauri", "target", "release");

const platformMap = {
  win32: { os: "windows", bin: "claws-cli.exe", ext: "zip" },
  linux: { os: "linux", bin: "claws-cli", ext: "tar.gz" },
  darwin: { os: "macos", bin: "claws-cli", ext: "tar.gz" },
};

const archMap = {
  x64: "x64",
  arm64: "arm64",
};

const platform = platformMap[process.platform];
if (!platform) {
  throw new Error(`unsupported platform: ${process.platform}`);
}

const arch = archMap[process.arch] ?? process.arch;
const binaryPath = path.join(targetDir, platform.bin);
if (!fs.existsSync(binaryPath)) {
  throw new Error(`missing binary: ${binaryPath}`);
}

const artifactBase = `claws-cli_${version}_${platform.os}_${arch}`;
const artifactPath = path.join(targetDir, `${artifactBase}.${platform.ext}`);

if (fs.existsSync(artifactPath)) {
  fs.rmSync(artifactPath, { force: true });
}

if (platform.ext === "zip") {
  const result = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-Command",
      `Compress-Archive -Path '${binaryPath}' -DestinationPath '${artifactPath}' -Force`,
    ],
    { stdio: "inherit" },
  );
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
} else {
  const result = spawnSync(
    "tar",
    ["-czf", artifactPath, "-C", targetDir, platform.bin],
    { stdio: "inherit" },
  );
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

console.log(`packaged ${path.basename(artifactPath)}`);
