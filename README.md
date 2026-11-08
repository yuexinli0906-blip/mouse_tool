# Yugutou Tool

一款基于 **Tauri + React + Rust** 构建的 Windows 桌面工具，专注于无线鼠标电量监控。

通过 HID 协议直接读取设备电量，无需依赖厂商控制软件常驻运行。采用模块化设备插件架构，将鼠标协议适配与应用核心功能解耦，让新设备的接入更加清晰、简单。

仓库：[wjyugutou/mouse_tool](https://github.com/wjyugutou/mouse_tool)

![UI](./docs/ui_home_final.png)

## S9 Pro DPI 与常规回报率（本地开发版）

选择左侧「前行者 S9 Pro」，在电量卡片下面的「性能设置」中点击已有 DPI 档位，或选择 125 / 250 / 500 / 1000 Hz。先关闭官方驱动；连接并唤醒鼠标后会自动读取，也可点击「读取设置」。只有设备写入后读回一致才显示「设置已核对」。错误时重新读取，不显示未经确认的设置。

本版只切现有 DPI 档位，不编辑 DPI 数值、灯光、按键或宏。回报率仅修改当前连接方式的字段。2.4G PID 为 S9 系列共用，使用者需确认连接的是 S9 Pro；存在多个候选接口时拒绝修改。原始设备数据读取失败时不会写入。

## ✨ 核心特性

- **🔋 电量监控**：通过 HID 协议读取支持设备的电量信息。
- **🖱️ 多设备支持**：目前支持雷柏 VT9 AIR、雷蛇毒蝰终极版（Viper Ultimate）、迈从 A5 V3 Ultra+、前行者 S9 Pro，以及 VGN S99 键盘。
- **📌 系统托盘**：在托盘中查看设备电量，快速切换设备与主窗口。
- **⚙️ 个性化设置**：支持主题、刷新间隔、关闭窗口行为及开机自启。
- **🔄 自动更新**：集成 Tauri Updater，通过 GitHub Releases 检查和安装新版本。
- **🧩 插件化架构**：采用构建期设备插件设计，将设备适配器、HID 协议实现与应用核心解耦，便于扩展更多鼠标型号。
- **📝 运行日志**：记录应用运行信息，便于问题排查与设备协议调试（优先写安装目录 `logs/mouse_tool.log`，无写权限则回退 AppData）。

## 🛠️ 技术栈

- **桌面框架**：[Tauri 2](https://tauri.app/)
- **前端**：[React](https://react.dev/) + TypeScript
- **后端**：[Rust](https://www.rust-lang.org/)
- **设备通信**：HID
- **包管理**：pnpm
- **持续集成与发布**：GitHub Actions

## 🚀 开发与运行

环境要求：Windows、Node.js（建议 LTS）、pnpm、Rust。

```powershell
pnpm install
pnpm dev:app
```

仅启动前端开发服务器（默认 http://localhost:1420）：

```powershell
pnpm dev
```

## 📦 下载安装

前往 [GitHub Releases](https://github.com/wjyugutou/mouse_tool/releases) 下载 Windows 安装包。

## 🔢 版本号

改 `package.json` 的 `version` 后执行：

```powershell
pnpm version:sync
```

会同步到 `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 与 `src-tauri/Cargo.lock`。`pnpm build` / `pnpm build:app` 也会先跑同步。

发版（工作区需干净）：

```powershell
pnpm release:bump 2.0.3
```

脚本会写入上述版本文件，提交 `chore: 版本 bump 至 2.0.3`，打 tag `v2.0.3`，并推送当前分支和这个 tag。

## 📁 本地打包

签名私钥放在项目目录（已 gitignore，勿提交）：

`mouse_tool_updater_keys/mouse_tool.key`

密码默认：`mousetool`

```powershell
pnpm build:app
```

产物在 `src-tauri/target/release/bundle/msi/`（当前只打 MSI，文件名形如 `yugutou_tool_{version}_x64_zh-CN.msi`）。

## 🚢 发版（自动更新）

1. `pnpm release:bump 2.0.3`：改版本、提交、打 tag `v2.0.3` 并推送
2. GitHub Actions `Release` 在收到 `v*` tag 后构建并上传 MSI、`latest.json`、`.sig`
3. Secrets：`TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

更新清单：

`https://github.com/wjyugutou/mouse_tool/releases/latest/download/latest.json`

**Latest Release 必须带** `latest.json`；空的 Latest 或长期 Draft 会导致应用内检查失败。

## 🧩 扩展设备支持

项目采用构建期设备插件架构。每款鼠标通过前端 `DeviceAdapter` 和对应的 Rust HID 实现接入，并通过统一的插件契约注册。

- 前端：`src/plugins/<id>/`（含 `manifest.json`）
- 后端：`src-tauri/src/devices/<id>.rs`，并在 `PLUGINS` 注册表追加一行

窗口、设置、托盘、日志、自动更新留在壳层。命令在 `generate_handler!` 里静态注册，不加载 DLL。电量只有一条壳层命令：`get_battery(device_id)`；`device_id` 与前端适配器的 `id` 来自同一份 `manifest.json`。

新增设备时，无需重写主窗口、设置页、托盘和自动更新等核心功能。详细说明请参阅 [设备插件开发指南](docs/plugin-development.md)。

### 已注册设备

| 插件 id | 名称 | 说明 |
| --- | --- | --- |
| `vt9-air` | VT9 AIR | 雷柏 VT9 AIR；被动中断读 HID 电量（Report `0xBB`）。不要对它用 GetInputReport / Feature，会卡鼠标 |
| `razer-viper-ultimate` | 毒蝰终极版 Viper Ultimate | 有线 `1532:007A` / 接收器 `1532:007B`；OpenRazer Feature（class `0x07` / cmd `0x80`）；充电状态同族命令 `0x84` |
| `mchose-a5-v3-ultra-plus` | 迈从 A5 V3 Ultra+ | 2.4G 无线 · 3955 平台 |
| `vgn-s99` | VGN S99 | 2.4G 无线机械键盘（首只 `keyboard` 分类）。接收器 `320F:5088` 的 `0xFF1C` 厂商接口，Output 报文 Report ID `0x04` + 命令 `0x1A` 查询，Input 中断读回。**不要用 Feature 报告**：该接口描述符无 `B1` 项，`HidD_SetFeature` 恒返回 `0x1`。HID 可能被 VGN VHUB 独占 |
| `qianxingzhe-s9-pro` | 前行者 S9 Pro | 2.4G 无线鼠标。VID `372E`（IPI/PIAO，`ms_pix_v1` 协议），接收器 `372E:103D` 的 `0xFF00` 厂商接口走 **Feature 报告**（Report ID `0x03`，payload 63 B），命令 `get_basic_info`（`0x4F/0x81`，首字节 CRC = `sum(payload[1..]) % 256`），**电量 = 回包 `buf[6]`**。首读可能只有 5 字节状态桩，必须重读；充电时该字段是哨兵值 `0xE4`(228)，拿不到百分比。**只读电量**，不做 DPI / 灯光 / 宏。只支持 S9 系列（Pro `0x1029`；Ultra `0x1027`、Max `0x105E` 会明确拒绝冒充）；2.4G 接收器 PID `1026/1033/103D/1055` 由三型号共用，无法区分型号 |

## 🏗️ 项目结构

- `src-tauri/src/plugin_sdk.rs` — 插件契约（`DevicePlugin` / `BatteryInfo`）
- `src-tauri/src/devices/` — 插件注册表与各鼠标的 HID 实现
- `src-tauri/src/tray.rs` — 托盘壳（设备列表来自插件表）
- `src-tauri/src/app_log.rs` — 文件日志
- `src-tauri/src/lib.rs` — 启动与命令注册
- `src/plugin-sdk/` — 前端契约与 `createDeviceAdapter`
- `src/plugins/` — 已安装的设备插件
- `src/devices/` — 壳层重新导出，页面继续从这里取注册表

## 📄 许可证

本项目采用 [MIT License](LICENSE) 开源。