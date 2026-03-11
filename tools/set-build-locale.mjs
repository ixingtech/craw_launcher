import fs from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const locale = process.argv[2];
const supportedLocales = new Set(["zh-CN", "en-US"]);

if (!supportedLocales.has(locale)) {
  throw new Error(`unsupported locale: ${locale}`);
}

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tsTarget = path.join(rootDir, "src", "lib", "buildLocale.ts");
const rustTarget = path.join(rootDir, "src-tauri", "src", "build_locale.rs");

fs.writeFileSync(tsTarget, `export const BUILD_LOCALE = "${locale}";\n`);
fs.writeFileSync(rustTarget, `pub const BUILD_LOCALE: &str = "${locale}";\n`);

console.log(`set build locale to ${locale}`);
