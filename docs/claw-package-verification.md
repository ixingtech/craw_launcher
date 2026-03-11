# `.claw` 龙虾包校验说明

## 1. `.claw` 是什么

- `.claw` 是小龙虾启动器使用的龙虾包格式。
- 它本质上是一个 ZIP 压缩包，只是把后缀改成了 `.claw`。
- 导入时，启动器只接受 `.claw` 后缀的文件。

## 2. 龙虾包里有什么

每个 `.claw` 龙虾包的根目录都包含一个 `manifest.json`，用于校验包内容是否被改动。

`manifest.json` 中至少包含这些字段：

- `formatVersion`
- `packageName`
- `exportedAt`
- `sourceDirName`
- `includeMemory`
- `includeAccountInfo`
- `entries`

其中 `entries` 会记录每个被导出文件的校验信息：

```json
{
  "path": "workspace/AGENTS.md",
  "size": 1234,
  "sha256": "..."
}
```

含义：

- `path`: 文件在龙虾包中的相对路径
- `size`: 文件大小
- `sha256`: 文件内容的 SHA-256 哈希值

## 3. 启动器会如何自动校验

导入 `.claw` 时，启动器会先做校验，再决定是否允许导入。

校验内容包括：

- `manifest.json` 是否存在
- `manifest.json` 是否能正常解析
- `formatVersion` 是否支持校验
- `manifest.json` 里登记的文件是否都存在
- ZIP 内是否出现了 `manifest.json` 未登记的额外文件
- 每个文件的 `size` 是否一致
- 每个文件的 `sha256` 是否一致

## 4. 校验失败时会发生什么

如果校验失败，启动器会弹出提示：

> 你正尝试导入的龙虾被篡改过，是否无视安全风险继续导入？

默认建议选择：

- `否`

只有你明确确认继续，启动器才会忽略风险继续导入。

## 5. 手动校验方式

如果你想自己检查一个 `.claw` 文件，可以按下面步骤操作。

### 方法 A：直接把 `.claw` 当 ZIP 解压

1. 复制一份 `.claw` 文件。
2. 把副本后缀从 `.claw` 改成 `.zip`。
3. 解压这个 ZIP。
4. 打开里面的 `manifest.json`。
5. 用 PowerShell 对照检查文件哈希。

### 方法 B：用解压工具直接打开

很多解压工具可以直接打开 `.claw`，因为它本质就是 ZIP。

## 6. Windows PowerShell 校验示例

假设你已经解压了龙虾包，目录结构如下：

```text
package/
  manifest.json
  workspace/
    AGENTS.md
```

你可以在 PowerShell 中执行：

```powershell
Get-FileHash .\workspace\AGENTS.md -Algorithm SHA256
```

得到的结果要和 `manifest.json` 中对应条目的 `sha256` 一致。

还要确认文件大小和 `manifest.json` 中记录的 `size` 一致。

## 7. 什么时候说明包被改过

出现以下任一情况，都说明这个 `.claw` 包已经被改动，或者至少不再可信：

- 缺少 `manifest.json`
- `manifest.json` 内容损坏
- 某个文件不存在
- 多出了 `manifest.json` 未登记的文件
- 文件大小不一致
- 文件 SHA-256 不一致

## 8. 使用建议

- 只导入你信任来源的 `.claw` 文件
- 校验失败时，默认不要继续导入
- 如果 `.claw` 来自别人，导入前先手动查看包内文件结构更稳妥
- 如果你关闭了“导出记忆”或“导出账号信息”，校验只负责检查内容是否被改动，不代表包内一定没有敏感信息；仍然要按来源判断风险
