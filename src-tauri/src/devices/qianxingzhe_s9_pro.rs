//! 前行者 S9 Pro 无线鼠标电量插件。
//!
//! **opt-in 厂商报文**：本模块使用 `0xFF00` 厂商接口的 **Feature 报告**
//! （`GET_FEATURE` / `SET_FEATURE`），Report ID `0x03`，payload 63 字节。
//! 不使用 `get_input_report`。
//!
//! ## 协议来源（2026-10-19 实机验证通过）
//!
//! VID `0x372E` 是 IPI（PIAO）的厂商号，前行者 S9 Pro 用的是同一套 OEM 固件，
//! 协议名 `ms_pix_v1`。协议来自官方 WebHID 配置器 `https://shan.ipigame.cn/`
//! 的 JS 包，并已由开源项目 `wkchew03/Mouse-Battery-Tracker`
//! （`mbt/drivers/ipi.py` + `docs/protocols.md`）在实机上验证。
//!
//! 本机实测（接收器 `372E:103D`，`iface=2`，`upage=0xFF00`）：
//!
//! ```text
//! 发送 03 22 50 00 02 4F 81 00 ...        （64 字节，buf[0] 是 Report ID）
//! 回包 03 50 00 0B 02 01 36 21 04 01 90 65 04 A5 5A 00
//!                 ^^ buf[6] = 0x36 = 54% 电量
//! ```
//!
//! ## 帧格式
//!
//! ```text
//! payload = [crc, 0x50, 0x00, 0x02, 0x4F, 0x81, 0, 0, ...]  补零到 63 字节
//! crc     = sum(payload[1..]) % 256，放在 payload[0]
//! ```
//!
//! 电量不是独立命令，而是 `get_basic_info` 回包的**第 5 个字段**（`buf[6]`）。
//!
//! ## 两个必须处理的坑
//!
//! 1. **首次读回可能是 5 字节状态桩** `03 50 02 00 02`，真正的 16 字节回包要再读几次。
//!    读一次就放弃，看起来和「设备不应答」一模一样。实测本机接收器首读即回真包，
//!    但错误命令码时确实只回状态桩，所以重读逻辑不能省。
//! 2. **充电时电量字段是哨兵值 `0xE4`(228)**，不是百分比。官方配置器此时也显示 228，
//!    谁都拿不到真实电量。此时上报「充电中 + 电量未知」，绝不能编一个数。
//!
//! ## 安全约束
//!
//! - 电量路径只查询；设置命令仅显式点击触发，读原值后最小写入并读回验证
//! - 最多重试有限次，不循环轰炸
//! - `get_input_report` 对任何设备都不许用（项目硬规则）

use crate::plugin_sdk::{parse_manifest, BatteryInfo, DevicePlugin};
use hidapi::{HidApi, HidDevice};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

/// IPI / PIAO 厂商号。
const VENDOR_ID: u16 = 0x372E;

/// 本插件认的 PID —— **只收前行者 EWEADN S9 系列**。
///
/// ## 为什么不能顺便收 IPI 自家机型
///
/// `0x372E` 是 IPI/PIAO 的厂商号，同厂商还有 IPI Float 88、PIAO 等机型
/// （PID `0x1014` / `0x1015` / `0x1028` / `0x1056`），协议同样是 `ms_pix_v1`。
/// 早期版本把这些也列了进来，后果是：插一只 **IPI Float 88** 会被这个
/// 「前行者 S9 Pro」插件认领，电量能读出来，但**设备名是错的**，
/// 详情里还会写出错误的「由 Pro/Ultra/Max 共用」说明。
///
/// 插件名承诺只支持 S9 Pro，就不该顺手支持别人的鼠标。要支持 IPI Float 88
/// 请另开一个插件（`ipi-float-88`），各自的 manifest 名字才诚实。
///
/// ## 型号与 PID 对照（来源：官方驱动 `D:\EWEADN S9\config.xml`）
///
/// | 型号 | 有线 PID | 2.4G 接收器 PID |
/// | --- | --- | --- |
/// | S9 Pro | `0x1029` | `0x1026` / `0x1033` / `0x103D` / `0x1055` |
/// | S9 Ultra | `0x1027` | `0x1026` / `0x1033` / `0x103D` / `0x1055` |
/// | S9 Max | `0x105E` | `0x1026` / `0x103D` / `0x1055` |
///
/// 所以：有线 PID 能唯一认出型号，2.4G 接收器 PID 认不出。
/// 本版只保证 S9 Pro；遇到能确认是 Ultra/Max 的有线 PID 时，明确拒绝说成 Pro。
const PRODUCT_IDS: &[u16] = &[
    0x1029, // S9 Pro 有线
    // Ultra / Max 的有线 PID 也列进来，才能「认出来并明确拒绝」，
    // 而不是枚举阶段就漏掉、被当成设备没插。
    0x1027, // S9 Ultra 有线
    0x105E, // S9 Max 有线
    0x1026, // S9 系列 2.4G（Pro/Ultra/Max 共用）
    0x1033, // S9 系列 2.4G（Pro/Ultra/Max 共用）
    0x103D, // S9 系列 2.4G（Pro/Ultra/Max 共用），本机实测
    0x1055, // S9 系列 2.4G（Pro/Ultra/Max 共用）
];

/// 能**唯一确认**是其它型号的有线 PID。命中就不是 Pro，不能冒充。
///
/// `0x1027` = S9 Ultra 有线，`0x105E` = S9 Max 有线（见 `config.xml`）。
const OTHER_MODEL_WIRED_PIDS: &[u16] = &[0x1027, 0x105E];

/// 这几个 2.4G PID 是 Pro/Ultra/Max 共用的，命中时型号不可判定。
const SHARED_RECEIVER_PIDS: &[u16] = &[0x1026, 0x1033, 0x103D, 0x1055];

/// S9 Pro 的有线 PID，命中即可确认型号。
const S9_PRO_WIRED_PID: u16 = 0x1029;

/// 厂商自定义接口的 usage page。标准鼠标接口（0x0001）读不到，且会被 Windows 独占。
const VENDOR_USAGE_PAGE: u16 = 0xFF00;

/// Feature 报告 ID，描述符里 `85 03`。
const REPORT_ID: u8 = 0x03;
/// Feature payload 长度，描述符 `95 3F` = 63 字节。
const PAYLOAD_LEN: usize = 63;
/// 含 Report ID 的完整帧长。
const FRAME_LEN: usize = PAYLOAD_LEN + 1;

/// 帧标记，回包里 `buf[1]` 必须是它。
const FRAME_MARKER: u8 = 0x50;

/// `get_basic_info` 的命令码与参数（放在 payload[4]、payload[5]）。
const CMD_BASIC_INFO_OPCODE: u8 = 0x4F;
const CMD_BASIC_INFO_PARAM: u8 = 0x81;
/// 回包里 `buf[4]` 必须等于它。
const CMD_BASIC_INFO_ECHO: u8 = 0x02;

/// 首次读回的状态桩长度。≤ 这个长度的都是桩，不是真回包。
const STUB_LEN: usize = 5;
/// 充电时电量字段的哨兵值。
const CHARGING_SENTINEL: u8 = 0xE4;

/// 读回包最多尝试几次。
const READ_ATTEMPTS: usize = 8;
/// 每次读之间的间隔。设备要一点时间才把真回包准备好。
const READ_GAP: Duration = Duration::from_millis(50);
/// 整个设备的读取加锁，避免首页和托盘同时打开同一个 HID 句柄。
static READ_LOCK: Mutex<()> = Mutex::new(());

const MANIFEST_JSON: &str = include_str!("../../../src/plugins/qianxingzhe-s9-pro/manifest.json");

pub static PLUGIN: DevicePlugin = DevicePlugin {
    manifest_json: MANIFEST_JSON,
    read_battery: read_s9_pro_battery,
};

/// 组一帧 `get_basic_info` 查询。`buf[0]` 是 Report ID，`buf[1..]` 是 63 字节 payload。
fn basic_info_request() -> [u8; FRAME_LEN] {
    let mut frame = [0u8; FRAME_LEN];
    frame[0] = REPORT_ID;
    // payload 从 frame[1] 开始，payload[0] 是 crc，先留 0
    frame[2] = FRAME_MARKER;
    frame[3] = 0x00;
    frame[4] = 0x02;
    frame[5] = CMD_BASIC_INFO_OPCODE;
    frame[6] = CMD_BASIC_INFO_PARAM;

    // crc = sum(payload[1..]) % 256，写回 payload[0] 即 frame[1]
    let sum = frame[2..].iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
    frame[1] = sum;
    frame
}

/// 解析一次读回。
///
/// 返回 `Ok(None)` 表示「这是状态桩，不是真回包，继续读」。
/// 返回 `Ok(Some(parse))` 表示拿到了 `basic_info` 回包。
fn parse_reply(resp: &[u8]) -> Result<Option<Reading>, String> {
    // 桩：3 或 5 字节，只有 0x50 标记但没有完整 payload
    if resp.len() <= STUB_LEN {
        return Ok(None);
    }
    if resp[1] != FRAME_MARKER {
        return Err(format!(
            "帧标记不是 0x50（实际 0x{:02X}）",
            resp.get(1).copied().unwrap_or(0)
        ));
    }
    if resp.len() <= 6 {
        return Err(format!("回包过短: {} 字节，取不到电量字段", resp.len()));
    }
    if resp[4] != CMD_BASIC_INFO_ECHO {
        return Err(format!(
            "命令码不符: 回包 buf[4]=0x{:02X}，期望 0x{CMD_BASIC_INFO_ECHO:02X}",
            resp[4]
        ));
    }

    let raw = resp[6];
    if raw == CHARGING_SENTINEL {
        // 充电中，厂商自己也没有百分比。上报「充电中 + 未知电量」。
        return Ok(Some(Reading {
            percent: None,
            charging: Some(true),
        }));
    }
    if raw <= 100 {
        return Ok(Some(Reading {
            percent: Some(raw),
            charging: None,
        }));
    }

    // 帧校验通过了，说明设备确实在应答，只是这个值不是电量。
    // 报「已连接但电量未知」，不能报成断连。
    Err(format!(
        "电量字段 {raw} 既不在 0–100 也不是充电哨兵值，无法判定"
    ))
}

/// 解析结果。
#[derive(Debug, PartialEq, Eq)]
struct Reading {
    percent: Option<u8>,
    charging: Option<bool>,
}

/// 型号判定结果。VID+PID 认不出 2.4G 接收器背后是哪一只。
#[derive(Debug, PartialEq, Eq)]
enum ModelGuess {
    /// 有线 PID 唯一命中 S9 Pro。
    Confirmed,
    /// 2.4G 接收器 PID 是 Pro/Ultra/Max 共用，只能确认是 S9 系列。
    SharedReceiver,
    /// 有线 PID 明确是 Ultra / Max，不是 Pro。
    OtherModel(u16),
    /// 不是 S9 系列的已知 PID（例如 IPI 自家机型）。本插件不该认领。
    NotS9Series(u16),
}

/// 按 PID 判型号。判定依据见 [`PRODUCT_IDS`] 的注释表格。
fn guess_model(pid: u16) -> ModelGuess {
    if pid == S9_PRO_WIRED_PID {
        ModelGuess::Confirmed
    } else if OTHER_MODEL_WIRED_PIDS.contains(&pid) {
        ModelGuess::OtherModel(pid)
    } else if SHARED_RECEIVER_PIDS.contains(&pid) {
        ModelGuess::SharedReceiver
    } else {
        // 不是 S9 系列。正常情况下枚举阶段就用 PRODUCT_IDS 过滤掉了，
        // 这里是兜底：万一将来 PRODUCT_IDS 被误加，也不会被当成 S9 蒙混过去。
        ModelGuess::NotS9Series(pid)
    }
}

fn read_s9_pro_battery() -> Result<BatteryInfo, String> {
    let _guard = READ_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let api = HidApi::new().map_err(|err| format!("hidapi 初始化失败: {err}"))?;
    let device_label = parse_manifest(MANIFEST_JSON).name;

    let mut enum_notes = Vec::new();
    let mut candidate = None;

    for device in api.device_list() {
        if device.vendor_id() != VENDOR_ID || !PRODUCT_IDS.contains(&device.product_id()) {
            continue;
        }
        let page = device.usage_page();
        let usage = device.usage();
        let iface = device.interface_number();
        let pid = device.product_id();
        let product = device.product_string().unwrap_or_default().to_string();

        enum_notes.push(format!(
            "iface={iface} page={page:04X} usage={usage:04X} pid={pid:04X} {product}"
        ));

        // 只开厂商自定义接口（0xFF00）。标准鼠标接口被 Windows 独占，打开就「拒绝访问」。
        if page == VENDOR_USAGE_PAGE {
            candidate = Some((pid, page, iface, device.path().to_owned(), product));
        }
    }

    let Some((pid, page, iface, path, product)) = candidate else {
        if enum_notes.is_empty() {
            return Ok(BatteryInfo::disconnected(
                device_label,
                "未找到前行者 S9 Pro 接收器。请插好 2.4G 接收器。".to_string(),
            ));
        }
        return Ok(BatteryInfo {
            percent: None,
            charging: None,
            connected: true,
            device: device_label,
            detail: format!(
                "已看到 {VENDOR_ID:04X} 设备，但没有 0xFF00 厂商接口。接口: {}",
                enum_notes.join("; ")
            ),
            unsupported: false,
            model_uncertain: false,
        });
    };

    let device = match api.open_path(&path) {
        Ok(device) => device,
        Err(err) => {
            return Ok(BatteryInfo::disconnected(
                device_label,
                format!("接收器打不开: {err}。请关闭鼠标官方驱动后重试（它会独占 HID 接口）。"),
            ));
        }
    };

    let model = guess_model(pid);

    // 不是 S9 系列（例如 IPI 自家机型）：本插件不该认领，也不能拿它的电量。
    // 采用 unsupported 而不是 disconnected：设备其实插着，说「未连接」会让用户去查线。
    if let ModelGuess::NotS9Series(other_pid) = model {
        return Ok(BatteryInfo::unsupported(
            device_label,
            format!(
                "发现 VID {VENDOR_ID:04X} 的 PID 0x{other_pid:04X}，属于 IPI 同厂其它机型，不属于前行者 S9 系列，本插件不认领。"
            ),
        ));
    }

    // 有线 PID 明确是 Ultra/Max：不能把别人的鼠标标成 S9 Pro。
    if let ModelGuess::OtherModel(other_pid) = model {
        let name = match other_pid {
            0x1027 => "S9 Ultra",
            0x105E => "S9 Max",
            _ => "同系列其它型号",
        };
        // 用 unsupported 而不是普通「无电量」：界面要直说「型号不支持」，
        // 否则用户会看到「等待鼠标唤醒」去查设备，那是误导。
        return Ok(BatteryInfo::unsupported(
            format!("前行者 {name}（未支持）"),
            format!(
                "检测到 {name}（有线 PID 0x{other_pid:04X}），本版只适配 S9 Pro，暂不读取电量。"
            ),
        ));
    }

    let device_name = if product.is_empty() {
        format!("前行者 S9 Pro {VENDOR_ID:04X}:{pid:04X} page={page:04X} iface={iface}")
    } else {
        format!("{product} page={page:04X} iface={iface}")
    };

    // 共用接收器 PID 时，协议能读，但「是不是 Pro」无法从 HID 确认。
    let model_note = match model {
        ModelGuess::SharedReceiver => {
            format!("（2.4G PID 0x{pid:04X} 由 Pro/Ultra/Max 共用，型号按 S9 系列处理）")
        }
        _ => String::new(),
    };
    // 主界面要能看见这件事，不能只写在折叠的诊断详情里（review 意见 #3）。
    let model_uncertain = model_is_uncertain(&model);

    match read_once(&device) {
        Ok(reading) => {
            let detail = match (reading.percent, reading.charging) {
                (Some(level), _) => {
                    format!(
                        "Feature 0x{REPORT_ID:02X}/0x{CMD_BASIC_INFO_OPCODE:02X} · {level}%{model_note}"
                    )
                }
                (None, Some(true)) => {
                    format!("正在充电，此状态拿不到百分比（厂商配置器同样只显示 228）{model_note}")
                }
                _ => format!("已连接接收器，但电量未知（{iface} 接口无有效回包）{model_note}"),
            };
            Ok(BatteryInfo {
                percent: reading.percent,
                charging: reading.charging,
                connected: true,
                device: device_name,
                detail,
                unsupported: false,
                model_uncertain,
            })
        }
        // 打不开 / 写不进去 = 此刻拿不到设备
        Err(ReadError::Write(err)) => Ok(BatteryInfo::disconnected(
            device_name,
            format!("写 Feature 报文失败: {err}。请拔插接收器，或关闭鼠标官方驱动后重试。"),
        )),
        // 写进去了但没读到真回包：设备在，只是电量未知，不能报 0%
        Err(ReadError::NoReply(err)) => Ok(BatteryInfo {
            percent: None,
            charging: None,
            connected: true,
            device: device_name,
            detail: format!(
                "已连接接收器，但没读到电量: {err}。请确认鼠标已开机并与接收器配对。{model_note}"
            ),
            unsupported: false,
            model_uncertain,
        }),
    }
}

/// 型号是否认不出来。共用 2.4G 接收器 PID 时为真；有线 PID 能唯一确认型号时为假。
fn model_is_uncertain(model: &ModelGuess) -> bool {
    matches!(model, ModelGuess::SharedReceiver)
}

/// 读电量的失败方式。壳层要区分「拿不到设备」和「设备没回答」。
enum ReadError {
    /// 写 Feature 报文失败，等同于拿不到设备。
    Write(String),
    /// 命令已发出，但没读到有效回包。
    NoReply(String),
}

/// 发一次查询，反复读到真回包为止。
fn read_once(device: &HidDevice) -> Result<Reading, ReadError> {
    let request = basic_info_request();
    if let Err(err) = device.send_feature_report(&request) {
        return Err(ReadError::Write(format!("{err}")));
    }

    let mut last_err = String::from("没有读到回包");

    for _ in 0..READ_ATTEMPTS {
        thread::sleep(READ_GAP);

        // 关键：缓冲区必须给满 64 字节。短缓冲设备会直接报 read error。
        let mut buf = [0u8; FRAME_LEN];
        buf[0] = REPORT_ID;

        match device.get_feature_report(&mut buf) {
            Ok(len) => {
                let resp = &buf[..len];
                match parse_reply(resp) {
                    // 状态桩，继续读
                    Ok(None) => {}
                    Ok(Some(reading)) => return Ok(reading),
                    Err(err) => last_err = err,
                }
            }
            Err(err) => last_err = format!("读回包失败: {err}"),
        }
    }

    Err(ReadError::NoReply(last_err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_the_plugin_identity() {
        let meta = PLUGIN.meta();
        assert_eq!(meta.id, "qianxingzhe-s9-pro");
        assert_eq!(meta.name, "前行者 S9 Pro");
        assert_eq!(meta.description, "2.4G 无线游戏鼠标");
    }

    /// 手算核对：crc = sum(payload[1..]) % 256。
    #[test]
    fn request_frame_has_valid_checksum() {
        let frame = basic_info_request();
        assert_eq!(frame[0], REPORT_ID);
        assert_eq!(frame[2], FRAME_MARKER);
        assert_eq!(frame[5], CMD_BASIC_INFO_OPCODE);
        assert_eq!(frame[6], CMD_BASIC_INFO_PARAM);

        let sum = frame[2..].iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
        assert_eq!(
            frame[1], sum,
            "payload[0] 必须是 payload[1..] 的和取低 8 位"
        );

        // 实机发出的前 8 字节是 `03 22 50 00 02 4F 81 00`，crc = 0x22
        assert_eq!(frame[1], 0x22, "实测 crc 是 0x22");
    }

    /// 实机抓到的真回包（54% 电量）。
    #[test]
    fn parses_real_capture() {
        let resp = [
            0x03, 0x50, 0x00, 0x0B, 0x02, 0x01, 0x36, 0x21, 0x04, 0x01, 0x90, 0x65, 0x04, 0xA5,
            0x5A, 0x00,
        ];
        let reading = parse_reply(&resp).unwrap().unwrap();
        assert_eq!(reading.percent, Some(0x36));
        assert_eq!(reading.percent, Some(54));
    }

    /// 5 字节状态桩必须被判成「继续读」，不能当成回包。
    #[test]
    fn stub_is_not_a_reading() {
        let stub = [0x03, 0x50, 0x02, 0x00, 0x02];
        assert_eq!(parse_reply(&stub).unwrap(), None);

        // 更短的也要是 None，不能 panic
        assert_eq!(parse_reply(&[0x03]).unwrap(), None);
        assert_eq!(parse_reply(&[]).unwrap(), None);
    }

    /// 充电时电量字段是 0xE4 哨兵值：报「充电中 + 未知电量」，不能编数字。
    #[test]
    fn charging_sentinel_yields_no_percent() {
        let resp = [
            0x03, 0x50, 0x00, 0x0B, 0x02, 0x01, 0xE4, 0x21, 0x04, 0x01, 0x90, 0x65, 0x04, 0xA5,
            0x5A, 0x00,
        ];
        let reading = parse_reply(&resp).unwrap().unwrap();
        assert_eq!(reading.percent, None);
        assert_eq!(reading.charging, Some(true));
    }

    /// 帧标记不对要报错，不能瞎解析。
    #[test]
    fn wrong_marker_is_rejected() {
        let resp = [0x03, 0x51, 0x00, 0x0B, 0x02, 0x01, 0x36];
        assert!(parse_reply(&resp).is_err());
    }

    /// 命令码不对要报错。实测错误命令码时设备只回状态桩。
    #[test]
    fn wrong_opcode_echo_is_rejected() {
        let resp = [0x03, 0x50, 0x00, 0x0B, 0x03, 0x01, 0x36];
        assert!(parse_reply(&resp).is_err());
    }

    /// 0–100 的边界值要正确解析。
    #[test]
    fn percent_boundaries() {
        for level in [0u8, 1, 50, 99, 100] {
            let mut resp = [0x00u8; 16];
            resp[0] = REPORT_ID;
            resp[1] = FRAME_MARKER;
            resp[4] = CMD_BASIC_INFO_ECHO;
            resp[6] = level;
            assert_eq!(parse_reply(&resp).unwrap().unwrap().percent, Some(level));
        }
    }

    /// 超出 0–100 又不是哨兵值：报错（上层转成「设备在但电量未知」）。
    #[test]
    fn out_of_range_value_is_an_error() {
        let mut resp = [0x00u8; 16];
        resp[0] = REPORT_ID;
        resp[1] = FRAME_MARKER;
        resp[4] = CMD_BASIC_INFO_ECHO;
        resp[6] = 200;
        assert!(parse_reply(&resp).is_err());
    }

    /// 有线 PID `0x1029` 唯一确认是 S9 Pro。
    #[test]
    fn wired_pro_pid_confirms_model() {
        assert_eq!(guess_model(0x1029), ModelGuess::Confirmed);
    }

    /// 2.4G 接收器 PID 由 Pro/Ultra/Max 共用，只能判成「共用」，不能当成 Pro 已确认。
    ///
    /// 依据：`D:\EWEADN S9\config.xml` 三个型号共用 1026/1033/103D/1055。
    #[test]
    fn shared_receiver_pids_are_not_confirmed() {
        for pid in [0x1026u16, 0x1033, 0x103D, 0x1055] {
            assert_eq!(
                guess_model(pid),
                ModelGuess::SharedReceiver,
                "0x{pid:04X} 是三个型号共用的接收器 PID，不能当作 Pro 已确认"
            );
        }
    }

    /// 有线 PID 明确是 Ultra / Max 时必须拒绝冒充 Pro。
    #[test]
    fn other_model_wired_pids_are_rejected() {
        assert_eq!(guess_model(0x1027), ModelGuess::OtherModel(0x1027)); // Ultra
        assert_eq!(guess_model(0x105E), ModelGuess::OtherModel(0x105E)); // Max
    }

    /// 所有 S9 系列可识别的 PID 都要在 PRODUCT_IDS 里，否则根本枚举不到
    /// （Ultra/Max 会被误报成「设备没插」，而不是「认出来了但不支持」）。
    #[test]
    fn every_s9_pid_is_enumerable() {
        for pid in [0x1029u16, 0x1027, 0x105E, 0x1026, 0x1033, 0x103D, 0x1055] {
            assert!(
                PRODUCT_IDS.contains(&pid),
                "PID 0x{pid:04X} 在 guess_model 里有含义，但不在 PRODUCT_IDS，枚举阶段就会被漏掉"
            );
        }
    }

    /// **回归测试**：不能顺手认领 IPI 自家的其它机型。
    ///
    /// `0x372E` 是 IPI/PIAO 的厂商号，同厂还有 IPI Float 88 / PIAO
    /// （PID `0x1014` / `0x1015` / `0x1028` / `0x1056`），协议同样是 `ms_pix_v1`。
    /// 早期版本把这些列进了 PRODUCT_IDS，结果插一只 IPI Float 88 会被
    /// 「前行者 S9 Pro」插件认领、显示错误设备名，详情里还会写出错误的型号说明。
    /// 插件名只承诺 S9 Pro，就不该顺手支持别人的鼠标。
    #[test]
    fn does_not_claim_other_ipi_models() {
        for pid in [0x1014u16, 0x1015, 0x1028, 0x1056] {
            assert!(
                !PRODUCT_IDS.contains(&pid),
                "PID 0x{pid:04X} 是 IPI 自家机型，不该被「前行者 S9 Pro」插件认领"
            );
            assert_eq!(
                guess_model(pid),
                ModelGuess::NotS9Series(pid),
                "PID 0x{pid:04X} 不是 S9 系列，必须判成 NotS9Series"
            );
        }
    }

    /// 本机实测的 `0x103D` 必须是 S9 系列（否则插着也读不到）。
    #[test]
    fn measured_receiver_pid_is_recognised() {
        assert!(PRODUCT_IDS.contains(&0x103D));
        assert_eq!(guess_model(0x103D), ModelGuess::SharedReceiver);
    }
}

// 设置协议来源：shan.ipigame.cn/app-Cn5AWPhI.js，wa/ul 控制器。
// 只切现有档位，保留所有 DPI 原始编码和颜色；不提供恢复出厂/批量写入。
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct S9Settings {
    dpi_stages: Vec<u32>,
    active_stage: u8,
    polling_rate: u32,
    supported_polling_rates: Vec<u32>,
}
struct SettingsSnapshot {
    public: S9Settings,
    profile: u8,
    raw_dpi: Vec<u8>,
    colors: Vec<u8>,
    rate_byte: u8,
}
fn settings_exchange(device: &HidDevice, bytes: &[u8], min_len: usize) -> Result<Vec<u8>, String> {
    let mut frame = [0u8; FRAME_LEN];
    frame[0] = REPORT_ID;
    frame[1..1 + bytes.len()].copy_from_slice(bytes);
    frame[1] = frame[2..].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    device
        .send_feature_report(&frame)
        .map_err(|e| format!("发送设置报文失败: {e}"))?;
    for _ in 0..READ_ATTEMPTS {
        thread::sleep(READ_GAP);
        let mut reply = [0u8; FRAME_LEN];
        reply[0] = REPORT_ID;
        let len = device
            .get_feature_report(&mut reply)
            .map_err(|e| format!("读取设置失败: {e}"))?;
        // 回包不含 checksum，0x50 是 frame[1]；第 3 字节（去 Report ID 后）回显序号。
        if len >= min_len
            && reply[0] == REPORT_ID
            && reply[1] == FRAME_MARKER
            && reply[4] == bytes[3]
        {
            return Ok(reply[1..len].to_vec());
        }
    }
    Err("鼠标未返回完整设置；请唤醒鼠标并关闭官方驱动后重试".into())
}
fn open_settings_device() -> Result<(HidDevice, bool), String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let candidates: Vec<_> = api
        .device_list()
        .filter(|d| {
            d.vendor_id() == VENDOR_ID
                && (d.product_id() == S9_PRO_WIRED_PID
                    || SHARED_RECEIVER_PIDS.contains(&d.product_id()))
                && d.usage_page() == VENDOR_USAGE_PAGE
        })
        .collect();
    if candidates.len() != 1 {
        return Err("需要且只能连接一只 S9 Pro 厂商接口（其它型号不支持设置）".into());
    }
    let info = candidates[0];
    Ok((
        api.open_path(info.path()).map_err(|e| e.to_string())?,
        SHARED_RECEIVER_PIDS.contains(&info.product_id()),
    ))
}
fn read_settings_snapshot(device: &HidDevice, wireless: bool) -> Result<SettingsSnapshot, String> {
    let sensor = settings_exchange(device, &[0, 80, 0, 10, 79, 64], 16)?;
    let profile = sensor[7];
    let count = (profile & 15) as usize;
    let active = (profile >> 4) as usize;
    if count == 0 || count > 8 || active == 0 || active > count {
        return Err("DPI 档位信息无效，拒绝写入".into());
    }
    let mut config = Vec::new();
    for page in 0..6u8 {
        let reply = settings_exchange(device, &[0, 80, 0, 0, 79, 65 + page], 16)?;
        if reply[4] != page + 1 {
            return Err("DPI 分页回包不匹配，拒绝使用旧数据".into());
        }
        config.extend_from_slice(&reply[if page == 0 { 6 } else { 5 }..15]);
    }
    let dpi_stages = config[..count * 2]
        .chunks_exact(2)
        .map(|b| {
            let raw = u16::from_le_bytes([b[0], b[1]]) as u32;
            if raw >= 30000 {
                raw
            } else {
                (raw + 1) * 50
            }
        })
        .collect();
    let rate_byte = sensor[6];
    let code = if wireless {
        rate_byte & 7
    } else {
        (rate_byte >> 4) & 7
    };
    let polling_rate = match code {
        0 => 1000,
        1 => 500,
        2 => 250,
        3 => 125,
        4 => 8000,
        5 => 4000,
        6 => 2000,
        _ => return Err("未知回报率编码".into()),
    };
    Ok(SettingsSnapshot {
        public: S9Settings {
            dpi_stages,
            active_stage: (active - 1) as u8,
            polling_rate,
            supported_polling_rates: vec![125, 250, 500, 1000],
        },
        profile,
        raw_dpi: config[..16].to_vec(),
        colors: config[16..40].to_vec(),
        rate_byte,
    })
}
#[tauri::command]
#[specta::specta]
pub async fn get_s9_settings() -> Result<S9Settings, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let _guard = READ_LOCK.lock().map_err(|_| "设备锁失效")?;
        let (device, wireless) = open_settings_device()?;
        Ok(read_settings_snapshot(&device, wireless)?.public)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
#[specta::specta]
pub async fn set_s9_settings(
    active_stage: Option<u8>,
    polling_rate: Option<u32>,
) -> Result<S9Settings, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if active_stage.is_some() == polling_rate.is_some() {
            return Err("每次只能修改一个设置".into());
        }
        let _guard = READ_LOCK.lock().map_err(|_| "设备锁失效")?;
        let (device, wireless) = open_settings_device()?;
        let before = read_settings_snapshot(&device, wireless)?;
        let mut command = if let Some(stage) = active_stage {
            if stage as usize >= before.public.dpi_stages.len() {
                return Err("DPI 档位越界".into());
            }
            vec![
                0,
                80,
                6,
                80,
                58,
                0,
                (((stage + 1) as u8) << 4) | (before.profile & 15),
                before.raw_dpi[stage as usize * 2],
                before.raw_dpi[stage as usize * 2 + 1],
                before.colors[stage as usize * 3],
                before.colors[stage as usize * 3 + 1],
                before.colors[stage as usize * 3 + 2],
            ]
        } else {
            let code = match polling_rate.unwrap() {
                1000 => 0,
                500 => 1,
                250 => 2,
                125 => 3,
                _ => return Err("仅支持 125/250/500/1000 Hz".into()),
            };
            // 仅修改当前连接对应的半字节，不覆盖另一个连接方式的配置。
            let rate = if wireless {
                (before.rate_byte & 0xf0) | code
            } else {
                (before.rate_byte & 0x0f) | (code << 4)
            };
            vec![0, 80, 1, 49, 53, 1, rate]
        };
        command[0] = 0;
        settings_exchange(&device, &command, 6)?;
        // 实机：切回报率后无线链路短暂重配，立即读会只收到状态桩。
        if polling_rate.is_some() {
            thread::sleep(Duration::from_millis(1000));
        }
        let after = read_settings_snapshot(&device, wireless)?;
        if after.raw_dpi != before.raw_dpi
            || after.colors != before.colors
            || after.public.dpi_stages.len() != before.public.dpi_stages.len()
            || active_stage.is_some_and(|s| {
                after.public.active_stage != s
                    || after.public.polling_rate != before.public.polling_rate
            })
            || polling_rate.is_some_and(|r| {
                after.public.polling_rate != r
                    || after.public.active_stage != before.public.active_stage
            })
        {
            return Err("写入后的读回校验不一致，停止操作，请重新读取设置".into());
        }
        Ok(after.public)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod settings_hardware_tests {
    #[test]
    #[ignore = "需要 S9 Pro 实机；切换后恢复原始档位和回报率"]
    fn settings_roundtrip_and_restore() {
        tauri::async_runtime::block_on(async {
            let original = super::get_s9_settings().await.unwrap();
            println!("原始设置: {:?}", original);
            let stage = (original.active_stage + 1) % original.dpi_stages.len() as u8;
            let rate = if original.polling_rate == 500 {
                1000
            } else {
                500
            };
            let result = async {
                let dpi = super::set_s9_settings(Some(stage), None).await?;
                assert_eq!(dpi.active_stage, stage);
                let polling = super::set_s9_settings(None, Some(rate)).await?;
                assert_eq!(polling.polling_rate, rate);
                Ok::<(), String>(())
            }
            .await;
            let restore_dpi = super::set_s9_settings(Some(original.active_stage), None).await;
            let restore_rate = super::set_s9_settings(None, Some(original.polling_rate)).await;
            assert!(restore_dpi.is_ok(), "恢复 DPI 失败: {restore_dpi:?}");
            let restored = restore_rate.expect("恢复回报率失败");
            assert_eq!(restored.active_stage, original.active_stage);
            assert_eq!(restored.polling_rate, original.polling_rate);
            assert_eq!(restored.dpi_stages, original.dpi_stages);
            result.expect("实机切换验证失败，原设置已恢复");
            println!("生产命令 DPI/回报率切换和恢复通过");
        });
    }
}
