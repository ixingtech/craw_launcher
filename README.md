# 小龙虾启动器

## 龙虾包安全校验

导出龙虾包时，压缩包根目录会附带一个 `manifest.json`，其中包含：

- 导出时间 `exportedAt`
- 龙虾包名称 `packageName`
- 每个已导出文件的相对路径 `path`
- 每个文件的大小 `size`
- 每个文件的 SHA-256 `sha256`

导入龙虾包时，启动器会先校验：

- `manifest.json` 是否存在
- `manifest.json` 是否可解析
- ZIP 内文件是否和 manifest 记录一致
- 每个文件的大小是否一致
- 每个文件的 SHA-256 是否一致

如果校验失败，启动器会提示：

`你正尝试导入的龙虾被篡改过，是否无视安全风险继续导入？`

默认应选择“否”。

## 手动检验方式

先解压龙虾包，然后把文件的 SHA-256 和 `manifest.json` 里的 `sha256` 对照。

Windows PowerShell 示例：

```powershell
Get-FileHash .\workspace\AGENTS.md -Algorithm SHA256
```

查看 `manifest.json` 中对应条目：

```json
{
  "path": "workspace/AGENTS.md",
  "size": 1234,
  "sha256": "..."
}
```

如果文件大小或 SHA-256 不一致，说明龙虾包内容已经被改动。
## macOS

See [docs/macos-build.md](docs/macos-build.md) for:

- default OpenClaw paths on macOS
- `.app` vs CLI executable selection
- macOS bundle commands: `pnpm build:mac`, `pnpm build:mac-app`, `pnpm build:mac-dmg`
- signing and notarization notes
