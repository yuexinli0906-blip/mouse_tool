---
name: develop
description: >-
  Use when changing mouse_tool code, adding a mouse/device, touching battery/tray/HID,
  or extending the frontend device list. Enforces build-time device plugin conventions.
---

# 开发 mouse_tool

改功能或加设备前先按下面的规则做。插件是**构建期的一只设备**（鼠标、键盘或耳机），不是运行时插件市场。给人看的步骤在 `docs/plugin-development.md`。

## 硬规则

1. **禁止运行时插件。** 不要 DLL 热加载，不要扫描用户目录发现插件。命令只在 `src-tauri/src/lib.rs` 的 `collect_commands!` 里静态注册（Tauri Specta 的 invoke handler）。
2. **壳层不要插件化。** 窗口、设置、托盘壳、日志、更新留在 app shell。只有「一只设备」才是设备插件。
3. **元数据只写一处。** `id` / `name` / `description` / `category` 只放在 `src/plugins/<device>/manifest.json`。前端直接 import 该文件；Rust 用 `include_str!` 读同一文件。`category` 只能是 `mouse`、`keyboard`、`headset`（界面：鼠标 / 键盘 / 耳机），名单在 `src/plugin-sdk/types.ts` 与 Rust `DeviceCategory`。禁止第二份设备表：不要恢复 `TRAY_DEVICES`，不要在设置默认值里再写死 `vt9-air`（默认设备从 `deviceRegistry` / `PLUGINS` 取第一项）。
4. **注册只有这两个列表。**
   - 前端：`src/plugins/index.ts` 的 `deviceRegistry`，一只设备一行 import。
   - Rust：`src-tauri/src/devices/mod.rs` 的 `PLUGINS`，外加 `src-tauri/src/devices/` 下的模块。
   - 托盘菜单和电量都走 `PLUGINS`，不要在 `tray.rs` 再列设备。菜单按鼠标 / 键盘 / 耳机分组，列出该分类里的每一只；某分类没有设备时显示「未选择」。悬停提示同样列出每一只的电量。名称仍从 `PLUGINS` 取。图标数字仍是 `trayDeviceId` 那一只。首页侧栏高亮仍是 `selectedByCategory`（每类至多一只）。
5. **电量命令。** 只用 `get_battery(device_id)`。前端是 `commands.getBattery(deviceId, fresh)`（`src/bindings.ts`）。禁止再加无参、隐含只支持 VT9 的 `get_battery`。
6. **HID 按设备区分，不要一刀切。** 先看报告描述符，再选通道。
   - 默认，以及雷柏 VT9 AIR：只允许中断 `read_timeout`。禁止 `GetInputReport`、Feature report、控制传输（会卡死雷柏鼠标）。
   - 雷蛇，或模块顶部写明 opt-in Feature 的插件：可以用 hidapi 的 `send_feature_report` / `get_feature_report`（Feature `SET_REPORT` / `GET_REPORT`）。不要把这条放开到 VT9，也不要改用 `GetInputReport`。
   - 描述符没有 Feature（`B1`）项、官方驱动用 `sendReport` 的设备：模块顶部写明 opt-in 厂商报文，用 Output `write` 加中断 `read_timeout`。迈从 A5 与 VGN S99 是这种。没有 `B1` 却发 Feature，Windows 会返回 `0x1`。不要用 `get_input_report`。
   - 壳层 capability 留在壳层，不要为了设备去放宽权限。

## 加一只设备

1. 复制一份前端目录，只改 `manifest.json`（`category` 填 `mouse` / `keyboard` / `headset`）。被动中断读复制 `vt9-air`。已确认必须用 Feature、并且会在 Rust 文件顶部写明 opt-in 的，复制 `razer-viper-ultimate`。走 Output 报文加 Input 读回的，复制 `mchose-a5-v3-ultra-plus` 或 `vgn-s99`。
2. 在 `src/plugins/index.ts` 的 `deviceRegistry` 加一行。
3. 新增 Rust 模块，文件名用下划线（例如 `vgn_s99.rs`），`manifest_json` 用 `include_str!` 指向上面的 `manifest.json`。默认只做中断 `read_timeout`。Feature 与 Output 报文都要在模块顶部写明 opt-in。VT9 永远不要用 Feature / `GetInputReport`。`devices/mod.rs` 的注册表测试写死了顺序和分类，新增后要同步改。
4. 在 `PLUGINS` 加一行。顺序与 `deviceRegistry` 保持一致。
5. 不要为了注册设备去改 `HomePage`。页面继续用 `src/devices` 导出的 `deviceRegistry`。

## 做完再停

- 必跑：`pnpm exec tsc --noEmit`、`pnpm exec eslint src`
- 改了 Rust 再跑：`cargo test --lib`
- 改了 Tauri 命令或事件的类型后跑 `pnpm bindings`，并提交 `src/bindings.ts`
