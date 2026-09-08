# AgentDeck 版本升级与本地打包安装操作手册

本指南适用于在本地 macOS 开发环境下，对 AgentDeck 进行版本号自增、生产构建（Release Build）以及安装覆盖到 `/Applications` 的完整流程。

---

## 目录
1. [三处版本号同步修改](#1-三处版本号同步修改)
2. [本地生产打包构建](#2-本地生产打包构建)
3. [安装覆盖到本地应用目录](#3-安装覆盖到本地应用目录)
4. [一键执行快捷命令](#4-一键执行快捷命令)
5. [常见问题排查 (FAQ)](#5-常见问题排查-faq)

---

## 1. 三处版本号同步修改

AgentDeck 采用前端（Node/Vite）+ 后端（Rust/Tauri）混合架构，版本发布前需保持以下三个配置文件的版本号一致：

| 文件路径 | 字段位置 | 示例 (以 `0.3.2` 为例) |
| :--- | :--- | :--- |
| `package.json` | `"version"` | `"version": "0.3.2"` |
| `src-tauri/tauri.conf.json` | `"version"` | `"version": "0.3.2"` |
| `src-tauri/Cargo.toml` | `[package] -> version` | `version = "0.3.2"` |

> **提示**：修改后可使用 `git diff` 快速核对三处版本号是否完全相同。

---

## 2. 本地生产打包构建

在项目根目录下执行生产构建命令：

```bash
npm run tauri:build
```

### 构建执行流程说明
1. **前置步骤**：自动触发 `beforeBuildCommand`（即 `npm run build && npm run generate:api-docs`），执行 TypeScript 类型检查、Vite 前端压缩打包、API 文档生成。
2. **后置步骤**：调用 Rust `cargo build --release` 编译原生二进制，并通过 Tauri 打包工具生成 macOS `.app` 应用程序包。

### 打包输出路径
- **macOS 应用包（用于直接安装）**：
  `src-tauri/target/release/bundle/macos/AgentDeck.app`

---

## 3. 安装覆盖到本地应用目录

构建完成后，将生成的 `.app` 复制覆盖至系统的 `/Applications/` 目录：

### 标准安装四步曲

```bash
# 1. 安全退出正在运行的 AgentDeck 实例
pkill -f AgentDeck || true

# 2. 清理旧版本应用程序
rm -rf /Applications/AgentDeck.app

# 3. 将新构建的 Release 包拷贝到系统应用程序目录
cp -R src-tauri/target/release/bundle/macos/AgentDeck.app /Applications/

# 4. 移除 macOS 隔离属性（避免未签名提示无法打开）
xattr -cr /Applications/AgentDeck.app

# 5. 启动全新版本应用
open /Applications/AgentDeck.app
```

---

## 4. 一键执行快捷命令

为提升日常更新与测试效率，可在 `package.json` 的 `scripts` 中配置快捷命令：

- **打包后直接安装覆盖**：
  ```bash
  npm run install:local
  ```
- **一键打包构建 + 自动安装启动**：
  ```bash
  npm run release:local
  ```

---

## 5. 常见问题排查 (FAQ)

### Q1: 提示 `Blocking waiting for file lock on build directory`？
- **原因**：后台有其他的 `cargo` 进程或 `npm run tauri:dev` 占用了 `target` 目录的文件锁。
- **解决办法**：关闭正在运行的开发模式进程（终端按 `Ctrl+C`），或执行：
  ```bash
  killall cargo tauri 2>/dev/null || true
  ```

### Q2: 打开应用时提示“已损坏，无法打开”或被 Gatekeeper 拦截？
- **原因**：本地自编译未接入 Apple Developer ID 证书签名，macOS 对本地覆盖的文件添加了隔离标签 `com.apple.quarantine`。
- **解决办法**：在终端运行清理命令：
  ```bash
  xattr -cr /Applications/AgentDeck.app
  ```

### Q3: 运行 `npm run tauri:build` 时报 SSL 证书错误（无法下载 crates.io 依赖）？
- **原因**：本地网络代理或环境证书链路拦截。
- **解决办法**：确保终端环境具备正常外网访问权限，或使用国内 crates 镜像源。
