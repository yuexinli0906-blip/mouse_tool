# 设备插件开发

给要加一只设备的人看。代理用的短清单在 `.cursor/skills/develop/SKILL.md`，规则以那边和本文一致为准。

## 插件是什么

这里的插件是**构建期的一只设备**（鼠标、键盘或耳机），不是可以下载安装的插件市场。

一只设备 = 一份前端适配器 + 一份 Rust HID 读取。两者在编译时写进程序。没有 DLL 热加载，也不扫描用户目录。

窗口、设置、托盘壳、日志、更新留在壳层，不要做成插件。托盘只负责图标和菜单，设备名单从插件表来。菜单按鼠标、键盘、耳机分组，列出该分类里的每一只；该分类没有设备时显示「未选择」。悬停提示同样列出每一只。图标数字仍跟着当前选中的那一只。

## 目录

| 作用                             | 路径                                                |
| -------------------------------- | --------------------------------------------------- |
| 元数据（唯一来源）               | `src/plugins/<id>/manifest.json`                    |
| 前端适配器                       | `src/plugins/<id>/index.ts`                         |
| 前端注册表                       | `src/plugins/index.ts` 的 `deviceRegistry`          |
| 前端类型与 `createDeviceAdapter` | `src/plugin-sdk/`                                   |
| 页面用的再导出                   | `src/devices/`（首页继续从这里取列表）              |
| Rust HID 实现                    | `src-tauri/src/devices/<name>.rs`（文件名用下划线） |
| Rust 契约                        | `src-tauri/src/plugin_sdk.rs`                       |
| Rust 注册表                      | `src-tauri/src/devices/mod.rs` 的 `PLUGINS`         |

`manifest.json` 有四个字段：`id`、`name`、`description`、`category`。`category` 取 `mouse`、`keyboard`、`headset`，对应首页一级分类「鼠标 / 键盘 / 耳机」。前端 `import` 这个文件，Rust 用 `include_str!` 读同一个文件。不要在托盘、设置默认值或别的表里再写一份 id / 名称。不要恢复 `TRAY_DEVICES`，也不要在设置里写死 `vt9-air`。默认托盘设备是两张注册表的第一项，所以两张表的顺序要一致。

电量只有一条壳层命令：`get_battery(device_id)`。`createDeviceAdapter` 通过生成绑定 `commands.getBattery` 调用它（定义在 `src/bindings.ts`）。不要再加无参、隐含只支持 VT9 的 `get_battery`。`list_devices` 返回 Rust 侧已注册的设备，用来对照 id。

现有五只：

- `vt9-air`：中断读，模板是 `src/plugins/vt9-air/` 与 `src-tauri/src/devices/vt9_air.rs`
- `razer-viper-ultimate`：Feature 报告，模板是 `src/plugins/razer-viper-ultimate/` 与 `src-tauri/src/devices/razer_viper_ultimate.rs`
- `mchose-a5-v3-ultra-plus`：迈从接收器 `3837:1018` 的 `0x4D` 输出/输入报文邮箱，模板是 `src/plugins/mchose-a5-v3-ultra-plus/` 与 `src-tauri/src/devices/mchose_a5_v3_ultra.rs`
- `vgn-s99`：**首只 `category: keyboard`**。接收器 `320F:5088` 的 `0xFF1C` 厂商接口走 Output 报文 + Input 中断读回（Report ID `0x04`，命令 `0x1A`），模板是 `src/plugins/vgn-s99/` 与 `src-tauri/src/devices/vgn_s99.rs`。它证明了一件事：**不是每只设备都能用 Feature 报告**。该接口描述符只有 `81 00`（Input）/ `91 00`（Output）、没有 `B1` 项，所以 `HidD_SetFeature` 恒返回 `0x1`
- `qianxingzhe-s9-pro`：**前行者 S9 Pro**，VID `372E` 是 IPI/PIAO 的厂商号，走 `ms_pix_v1` 协议。接收器 `372E:103D` 的 `0xFF00` 厂商接口**有 `B1` 项**，所以能用 Feature 报告（Report ID `0x03`，payload 63 B）。命令 `get_basic_info`：`payload = [crc, 0x50, 0x00, 0x02, 0x4F, 0x81, 0...]`，`crc = sum(payload[1..]) % 256`；**电量 = 回包 `buf[6]`**。两个坑：①首次读回可能只有 5 字节状态桩 `03 50 02 00 02`，真回包要重读几次，读一次就放弃会和「设备不应答」完全一样；②充电时电量字段是哨兵值 `0xE4`(228)，官方配置器此时也只显示 228，谁都拿不到百分比。模板是 `src/plugins/qianxingzhe-s9-pro/` 与 `src-tauri/src/devices/qianxingzhe_s9_pro.rs`。它证明了两件事：
  - **VID 不等于品牌**：同一个 VID 下 IPI 系几只鼠标共用一套协议，`0x372E` 在某些表里还被标成 `bytech` / PIAO
  - **协议一样不等于该认领**：同厂的 IPI Float 88 / PIAO（PID `0x1014` / `0x1015` / `0x1028` / `0x1056`）也是 `ms_pix_v1`，但**不要**把它们加进本插件的 `PRODUCT_IDS`。否则插一只 IPI Float 88 会被「前行者 S9 Pro」认领，设备名全错，详情里的型号说明也是错的。插件名承诺支持谁，`PRODUCT_IDS` 就只收谁；同厂其它机型另开插件（如 `ipi-float-88`）。回归测试 `does_not_claim_other_ipi_models` 守着这条
  - 型号与 PID 对照见官方 `D:\EWEADN S9\config.xml`：S9 Pro 有线 `0x1029`；Ultra `0x1027`；Max `0x105E`；三者共用 2.4G 接收器 PID `1026` / `1033` / `103D` / `1055`。**所以 2.4G 接收器上认不出具体型号**，只标「S9 系列」，Ultra/Max 有线 PID 明确拒绝冒充 Pro

## 加一只设备（鼠标 / 键盘 / 耳机）

1. 复制一份前端目录。普通鼠标复制 `src/plugins/vt9-air/`。已经确认必须用 Feature 报告、并且会在 Rust 文件顶部写明 opt-in 的，复制 `src/plugins/razer-viper-ultimate/`。
2. 只改新目录里的 `manifest.json`（`id` 用稳定的短横线，例如 `razer-viper-ultimate`；`category` 按设备类型填 `mouse` / `keyboard` / `headset`）。`index.ts` 保持 `createDeviceAdapter(manifest)`，把注释里的 Rust 路径改成新文件。
3. 在 `src/plugins/index.ts` 的 `deviceRegistry` 加一行 import。不要为了注册去改 `HomePage`。
4. 在 `src-tauri/src/devices/` 加模块，文件名用下划线。`manifest_json` 用 `include_str!` 指向第 2 步那个 `manifest.json`。
5. 在 `src-tauri/src/devices/mod.rs` 声明 `mod`，并在 `PLUGINS` 加一行。顺序与 `deviceRegistry` 相同。
6. 命令不用新注册。`get_battery` 已经在 `collect_commands!` 里，按 `device_id` 分发。改了命令签名后跑 `pnpm bindings`，提交生成的 `src/bindings.ts`。
7. 注意 `devices/mod.rs` 的单元测试断言了 `PLUGINS` 前几项的顺序和分类。**新增设备后这条测试会红，必须同步改**，否则 `cargo test --lib` 过不了。

### HID 通道的三种形态（选错会白干）

加新设备时，先确认它用哪种通道，**不要默认套用某一只的写法**：

| 形态                     | 特征                                        | 例子                                              |
| ------------------------ | ------------------------------------------- | ------------------------------------------------- |
| 被动中断读               | 设备自己周期性推报告                        | 雷柏 VT9 AIR（Report `0xBB`）                     |
| Feature 报告             | 描述符里**有 `B1` 项**                      | 雷蛇毒蝰（OpenRazer 协议）                        |
| Output 报文 + Input 读回 | 描述符里是 `81 00` + `91 00`，**没有 `B1`** | 迈从 A5 V3（`0x4D` 邮箱）、VGN S99（`0x1A` 邮箱） |

**判据**：读设备报告描述符，看有没有 `B1`（Feature）项。没有 `B1` 却调用 `send_feature_report`，Windows 会返回 `0x1`（函数不正确），很容易误以为是设备被占用或命令写错。VGN S99 就踩过这个坑。

**另一个坑**：从厂商 Electron 驱动逆向协议时，同一份驱动里往往有**多个设备类**（按 `customDeviceType` / `productName` 分派）。必须定位到目标机型对应的那一个类，抓到第一个同名函数（如 `getBattery`）就当结论会出错。VGN S99 的协议在 `WsServe` 类里，而驱动里还有另外几个类也定义了 `getBattery`，用的是完全不同的命令。

## HID 怎么读

默认只做中断 `read_timeout`，设备自己推报告时再解析。雷柏 VT9 AIR 必须停在这一档：官方软件挂起中断读后，设备大约每 3 秒推一次 Report `0xBB`，电量在第 7 字节。对它使用 `GetInputReport`、Feature report 或控制传输会把鼠标卡死。VT9 的实现里不要出现 `send_feature_report`、`get_feature_report` 或 `get_input_report`。

雷蛇毒蝰终极版没有这种被动推送。OpenRazer 的电量查询是 Feature `SET_REPORT` 再 `GET_REPORT`：90 字节报告，Report ID `0x00`，transaction `0xFF`，class `0x07`，电量命令 `0x80`，原始值在 `arguments[1]`（0–255，再换成百分比）。充电状态是同族命令 `0x84`。本仓库用 hidapi 的 `send_feature_report` / `get_feature_report` 做这件事，对应有线 `1532:007A` 和接收器 `1532:007B`。这只允许写在模块顶部已经声明 opt-in Feature 的插件里。`GetInputReport`（`get_input_report`）对谁都不许用，包括雷蛇。

新鼠标如果文档只有 Feature / 控制传输，而你还不能证明它对这只设备安全，就不要照搬雷蛇的发送代码。先把设备和「为什么不能读」写清楚，确认协议后再 opt-in。

迈从 A5 V3 Ultra+(3955) 是第三种情况：它的 `0xFF01` 集合上 `get_feature_report(0x11)` 在 Windows 恒失败（`0x1F`），但 `0x4D` 的 64 字节输出报文有回包。M HUB 对 3955 平台用的是 `[0x4D, 0x01, flags, len, cmd_lo, cmd_hi, biz, seq, data..., checksum]` 邮箱，`0x0900` 返回设备信息，电量在 data[12]、充电在 data[11]。首包可能是 `FF` 占位，要重发。模块顶部已写明这是 opt-in 的厂商报文；旧固件的 `0x11/0x06` Feature 只作为回退，不要把它当成 3955 的主通道，也不要用 `get_input_report`。

## 怎么验证

改完先在仓库根目录跑：

```powershell
pnpm exec tsc --noEmit
pnpm exec eslint src
```

动了首页电量卡片（`src/components/BatteryCard.tsx`）再跑：

```powershell
pnpm run check:card
```

这个脚本从 `BatteryCard.tsx` 源码里直接抽取 `levelFromInfo` / `statusText` / `hintText`
三个纯函数执行并断言文案，覆盖 6 个场景（充电有/无百分比、型号不支持、正常、
已连接无电量、未连接）。项目没有前端测试框架，这是那三个纯函数的回归闸门——
**它们的判断顺序直接决定用户看到什么**，顺序写反过一次：先判 `percent == null`
再判 `charging`，导致 S9 Pro 充电时显示「等待电量数据」，用户以为读不到电量。

改了这三个函数的顺序或文案，要同步更新脚本里的期望值；新增「无电量但有语义」的
状态（像 `unsupported`）也要补一个场景。

动了 Rust 再跑：

```powershell
cargo test --lib
```

改了命令或事件的类型后，再跑 `pnpm bindings`，把 `src/bindings.ts` 一起提交。

在 `src-tauri` 目录执行。解析器要有单元测试，不要只靠插上鼠标才知道包格式对不对。

实机在 Windows 上确认：

- VT9 AIR：插上雷柏接收器，选中后应出现百分比。读的过程里不要改用 Feature。
- 毒蝰终极版：先关掉 Razer Synapse（它可能独占 HID），插上接收器或用线连接，选「毒蝰终极版 Viper Ultimate」。应出现百分比；说明里会有 `raw n/255`。拔掉后再刷新，应提示未找到设备。
- 迈从 A5 V3 Ultra+：接收器 `3837:1018` 插在 MagDock 上也可以；选中后应出现百分比，说明里会写 `接收器邮箱 0x4D/0x0900`。M HUB 开着时如果读不到，先关掉再试。
- 两只都接上时，来回切换，确认 VT9 仍是中断读、毒蝰仍是 Feature 读。
- VGN S99：先**完全退出 VGN VHUB**（它会独占 `0xFF1C` 接口），选中「VGN S99」后应出现百分比，说明里会写 `厂商邮箱 Output 0x04/0x1A`。充电时插线应显示「充电中」。读不到时先确认键盘已开机并与接收器配对。
- 前行者 S9 Pro：插上 `372E:103D` 接收器，选中「前行者 S9 Pro」后应出现百分比，说明里会写 `Feature 0x03/0x4F · n%`。充电时插线只能显示「正在充电，此状态拿不到百分比」（厂商哨兵值 `0xE4`，官方软件同样拿不到）。**不能只跑探针验收**，要跑插件自己的读取路径：

  ```bash
  cd src-tauri
  cargo test --lib s9_pro_reads_real_hardware -- --ignored --nocapture
  ```

  这条是 `#[ignore]` 的实机测试，默认不跑（依赖硬件）。它会打印 `connected / percent / charging / detail`，并断言「有百分比时 ≤ 100；没百分比时必须是充电中」，防止把读取失败悄悄报成正常。

- 探针复现协议用 `tools/hid-probe`：`hid-probe s9pro` 会发同一条 `0x4F/0x81` 查询并把每次读回的原始字节打出来。改协议时先用它看真实报文，再改进插件。
