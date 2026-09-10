//! system_load/tray_icon.rs
//! 在 Windows / Linux 托盘图标上**手绘**系统指标文字（v0.7.6 引入）。
//!
//! ## 背景与动机（v0.7.6 修复）
//! v0.7.5/v0.7.6 初版的状态栏仅调用 `tray.set_title(...)`，该 API 在：
//! - **macOS**：原生支持，菜单栏右侧会渲染 title 文字 ✅
//! - **Windows**：`tray-icon` crate 把 title 写入 `NOTIFYICONDATAW::szTip`（**仅悬停 tooltip**），
//!   Windows 通知区域**不**支持图标旁持久文字标签 ❌
//! - **Linux**：取决于 DE，多数也不渲染 ❌
//!
//! 因此非 mac 平台启用状态栏后用户**完全看不到**任何数字。
//!
//! ## 方案
//! 在 Windows / Linux 上，每 tick 把指标文字**画进 32x32 RGBA 图标**并 `tray.set_icon(...)`，
//! 关闭时恢复默认品牌图标；macOS 维持原有 `set_title` 不变。
//! 完整指标字符串同时通过 `tray.set_title(...)` 写入 tooltip（Windows 悬停可见），
//! 弥补图标内文字因画布限制必须缩写的信息损失。
//!
//! ## 画布约束（重要，定义「不再适用的旧约定」）
//! - 通知区域实际显示 ~16-24px（来自 32x32 缩放），**最多 3 字符 / 行**（2x 位图字 = 10px/字符）。
//! - 高度只够 **2 行**（14px/行 + 2px 行距 + 边距 = 32）。
//! - 因此 Windows 托盘只能显示**缩写**指标；macOS 仍能显示完整 `C45 M60 ↓1.2K ↑0K`。
//! - MEM 指标在非 mac 平台本就为 0/禁用，故图标永不显示内存（与 UI 灰态一致）。
//!
//! ## 字体
//! 手写 5x7 位图字体，**不引入新 crate**（与 network.rs 手写 FFI 的风格一致）。
//! 覆盖：0-9, C, M, K, B, G, '.', ' ', '↓', '↑'（18 字形，状态栏所有可能字符）。
//! 未知字符降级为空白。
//!
//! ## 旧约定失效
//! ❌ 以为 `tray.set_title` 在所有平台都可见 → ✅ 非 mac 必须 set_icon 画进图标。

use tauri::image::Image;

use crate::system_load::metrics::{MetricsSnapshot, TrayTitleConfig};

const W: u32 = 32;
const H: u32 = 32;
const SCALE: u32 = 2;
const GLYPH_W: u32 = 5;
const GLYPH_H: u32 = 7;
/// 深色徽章底（slate-800）
const BG: [u8; 4] = [0x1E, 0x29, 0x3B, 0xFF];
/// 白色前景文字
const FG: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

/// 5x7 位图字形：7 个字节，每字节 = 1 行，bit0 = 最左像素
type Glyph = [u8; GLYPH_H as usize];

fn glyph(ch: char) -> Option<Glyph> {
    Some(match ch {
        '0' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1E, 0x01, 0x01, 0x0E, 0x01, 0x01, 0x1E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x0E, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x01, 0x0E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'M' => [0x11, 0x1B, 0x15, 0x11, 0x11, 0x11, 0x11],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0E],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        // ↓ 上 4 行杆 + 下三角箭头头
        '↓' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x0E, 0x1F],
        // ↑ 上三角箭头头 + 下 4 行杆
        '↑' => [0x1F, 0x0E, 0x04, 0x04, 0x04, 0x04, 0x04],
        _ => return None,
    })
}

/// 将 v0.7.6 的 `MetricsSnapshot` + `TrayTitleConfig` 格式化为**最多 2 行**的缩写文本，
/// 供 32x32 托盘图标渲染。每行 ≤3 字符（2x 缩放下硬约束）。
///
/// - CPU 行：`C{nn}`（百分位封顶 99 → 3 字符内）
/// - NET 行：取 rx/tx 较大方向 → `↓{2字符值}` / `↑{2字符值}`
///   值策略：<10 → `5B`；10-99 → `{:02}`（无单位，tooltip 补全）；K/M 范围同理
pub fn format_icon_lines(snap: &MetricsSnapshot, cfg: &TrayTitleConfig) -> Vec<String> {
    let mut lines: Vec<String> = Vec::with_capacity(2);
    if cfg.show_cpu {
        let pct = ((snap.cpu_usage * 100.0).round() as u32).min(99);
        lines.push(format!("C{}", pct));
    }
    if cfg.show_net {
        let (v, arrow) = if snap.net_rx_bps >= snap.net_tx_bps {
            (snap.net_rx_bps, '↓')
        } else {
            (snap.net_tx_bps, '↑')
        };
        lines.push(format_net_value(v, arrow));
    }
    // 安全：cpu + net 至多 2 行
    lines.truncate(2);
    lines
}

/// 始终产出 **3 字符**（arrow + 2 字符值）；尾部截断保证不溢出图标。
fn format_net_value(v: f64, arrow: char) -> String {
    if !v.is_finite() || v < 0.0 {
        return format!("{}0B", arrow);
    }
    let two: String = if v < 1024.0 {
        let n = (v.round() as u64).min(99);
        if n < 10 {
            format!("{}B", n) // 1 位 + B = 2 字符
        } else {
            format!("{:02}", n) // 2 位数字（无 B，tooltip 补全）
        }
    } else if v < 1024.0 * 1024.0 {
        let n = ((v / 1024.0).round() as u64).min(99);
        if n < 10 {
            format!("{}K", n)
        } else {
            format!("{:02}", n) // 2 位数字（无 K，tooltip 补全；49K→49）
        }
    } else if v < 1024.0 * 1024.0 * 1024.0 {
        let n = ((v / 1024.0 / 1024.0).round() as u64).min(99);
        if n < 10 {
            format!("{}M", n)
        } else {
            format!("{:02}", n)
        }
    } else {
        let n = ((v / 1024.0 / 1024.0 / 1024.0).round() as u64).min(99);
        if n < 10 {
            format!("{}G", n)
        } else {
            format!("{:02}", n)
        }
    };
    // 防御：确保值部分恰好 2 字符（截尾）
    let two: String = two.chars().take(2).collect();
    format!("{}{}", arrow, two)
}

/// 把 ≤2 行文字渲染到 32x32 RGBA 图标，返回 Tauri `Image`（owned，`'static`）。
/// 背景填充深色；未知字符降级为空格。
pub fn render_tray_icon(lines: &[&str]) -> Image<'static> {
    let mut buf = vec![0u8; (W * H * 4) as usize];
    // 1) 背景填充
    for px in buf.chunks_exact_mut(4) {
        px.copy_from_slice(&BG);
    }
    // 2) 文字（最多 2 行，每行 ≤3 字符，居中）
    let char_px = GLYPH_W * SCALE; // 10
    let line_px = GLYPH_H * SCALE; // 14
    let gap = 2u32;
    let top_margin = 1u32;
    for (i, line) in lines.iter().take(2).enumerate() {
        let n = line.chars().count() as u32;
        if n == 0 || n > 3 {
            continue;
        }
        let line_w = n * char_px;
        let x0 = (W - line_w) / 2;
        let y0 = top_margin + (line_px + gap) * i as u32;
        for (ci, ch) in line.chars().enumerate() {
            let Some(g) = glyph(ch) else { continue };
            let cx = x0 + ci as u32 * char_px;
            for r in 0..GLYPH_H {
                let row_bits = g[r as usize];
                for c in 0..GLYPH_W {
                    if row_bits & (1 << c) == 0 {
                        continue;
                    }
                    for dy in 0..SCALE {
                        for dx in 0..SCALE {
                            let x = cx + c * SCALE + dx;
                            let y = y0 + r * SCALE + dy;
                            let idx = (y as usize * W as usize + x as usize) * 4;
                            buf[idx..idx + 4].copy_from_slice(&FG);
                        }
                    }
                }
            }
        }
    }
    Image::new_owned(buf, W, H)
}
