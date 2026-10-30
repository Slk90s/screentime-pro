//!
//! window_bounds.rs
//! 「窗口恢复位置还在不在可见区」的公共判定 —— 悬浮指标条与桌宠共用（v1.2.12 整改 1.3 抽出）。
//!
//! 为什么要抽出来：
//! 这套判定原本只有 `float_window.rs` 一份私有实现（v1.2.1 为「副屏拔掉后状态栏消失」而加），
//! 而 **桌宠窗完全没防**：它的位置同样存在前端 localStorage（`screentime-pet` 的 `position`），
//! 同样是「绝对坐标 + 换机器/拔副屏后失效」。用户把桌宠拖到副屏再拔掉，重启后桌宠就落在
//! 已经不存在的屏幕上，表现为「桌宠开关是开的，但桌面上没有宠物」—— 与 v1.2.1 那次**同一个根因**。
//!
//! 判定口径（保持与 v1.2.1 一致，两轴各要求 1/3 边长的重叠）：
//! - 全部用**物理像素**：多屏混合 DPI 下逻辑/物理换算最容易出错，而显示器边界本身就是物理的；
//! - 只要与**任一**显示器在两轴上的重叠都 ≥ 边长的 1/3，就认为仍在可见区，不动用户位置
//!   （贴边摆放、被任务栏挡一半都不算丢，避免把用户好不容易摆好的位置改回去）；
//! - 否则按 `FallbackCorner` 贴到主屏角落（拿不到 primary 就用最大那块屏）。
//!
//! 修改历史：
//!   - 2026-10-29 @v1.2.12: 新增 - 从 `float_window.rs::clamp_to_visible_area` 抽出为公共
//!     util，补上**垂直轴**重叠判定（旧实现只看水平，桌宠这种高窗口会漏判），
//!     并给桌宠接上屏外回落（右下角，与 `petStore.defaultPosition()` 的默认位一致）。
//!     判定拆成 `monitor_rects` / `primary_rect` / `offscreen_target` 三块可测的纯函数，
//!     `clamp_to_visible_area` 只是它们的组合 + 一次 `set_position`：批次 2 的拖拽循环收尾
//!     需要「我已经知道位置，只问该不该纠偏」，而查询 `outer_position()` 可能拿到主线程
//!     尚未应用的历史值（见 `pet/drag.rs` 的线程约束说明）。
//!

use tauri::{PhysicalPosition, WebviewWindow};

/// 屏外时回落到主屏的哪个角。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackCorner {
    /// 悬浮指标条：右上角（v1.2.1 起的既有行为）
    TopRight,
    /// 桌宠：右下角（`petStore.defaultPosition()` 就是右下偏内 80px）
    BottomRight,
}

/// 物理像素矩形。x/y 可为负（多屏布局里副屏常在主屏左侧/上方，坐标系原点在主屏左上）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// `const`：测试里要用它构造 `const MON: &[Rect]` 这样的静态显示器表。
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    fn right(&self) -> i32 {
        self.x.saturating_add(self.w)
    }
    fn bottom(&self) -> i32 {
        self.y.saturating_add(self.h)
    }
}

/// 两轴各自至少要重叠边长的 1/3 才算「看得见」。
const MIN_OVERLAP_NUM: i32 = 1;
const MIN_OVERLAP_DEN: i32 = 3;

/// 一维重叠长度（区间相交，负数表示完全不交）。
fn overlap_1d(a_start: i32, a_end: i32, b_start: i32, b_end: i32) -> i32 {
    a_end.min(b_end) - a_start.max(b_start)
}

/// 窗口是否落在**某一块**显示器的可见区内（两轴各 ≥ 1/3 边长）。
///
/// 显示器列表为空时返回 `false` 是刻意选择：调用方拿不到任何屏幕信息时，
/// 「不动位置」和「挪回角落」都可能是错的，但枚举失败通常意味着显示服务临时不可用
/// （锁屏 / 远程桌面刚接上），此时把窗口拉回主屏角落比让它继续待在屏幕外更容易恢复。
pub fn rect_visible(win: Rect, monitors: &[Rect]) -> bool {
    monitors.iter().any(|m| {
        let min_x = (win.w * MIN_OVERLAP_NUM) / MIN_OVERLAP_DEN;
        let min_y = (win.h * MIN_OVERLAP_NUM) / MIN_OVERLAP_DEN;
        let min_x = min_x.max(1);
        let min_y = min_y.max(1);
        overlap_1d(win.x, win.right(), m.x, m.right()) >= min_x
            && overlap_1d(win.y, win.bottom(), m.y, m.bottom()) >= min_y
    })
}

/// 屏外时的回落位（左上角物理坐标）。`margin` 是该角两侧各留的物理像素。
pub fn fallback_position(
    primary: Rect,
    win: Rect,
    corner: FallbackCorner,
    margin: i32,
) -> (i32, i32) {
    // 目前两个回落角都在右缘，x 不分支；将来加左角时这里才需要按 corner 分叉。
    let x = primary.x + primary.w - win.w - margin;
    let y = match corner {
        FallbackCorner::TopRight => primary.y + margin,
        FallbackCorner::BottomRight => primary.y + primary.h - win.h - margin,
    };
    (x, y)
}

/// 枚举所有显示器的**物理像素**矩形；空列表时退化到 primary → current。
///
/// 一块屏都拿不到时返回 `Err`（不是空 Vec）：调用方需要能区分「屏内」「屏外」「没法判断」，
/// 第三种情况绝不能挪窗口。
pub fn monitor_rects(window: &WebviewWindow) -> Result<Vec<Rect>, String> {
    let mut monitors: Vec<Rect> = match window.available_monitors() {
        Ok(list) => list.iter().map(to_rect).collect(),
        Err(e) => {
            tracing::warn!("枚举显示器出错，改用退化路径: {e}");
            Vec::new()
        }
    };
    // 刚接上/拔掉显示器时 `available_monitors` 可能返回空列表，这时至少试试 primary 再试 current。
    if monitors.is_empty() {
        let fallback = window
            .primary_monitor()
            .ok()
            .flatten()
            .or_else(|| window.current_monitor().ok().flatten());
        if let Some(m) = fallback {
            monitors.push(to_rect(&m));
        }
    }
    if monitors.is_empty() {
        return Err("无法枚举任何显示器".to_string());
    }
    Ok(monitors)
}

/// 回落基准屏：primary 优先，拿不到就用**面积最大**那块（多屏机器上通常就是内置屏）。
pub fn primary_rect(window: &WebviewWindow, monitors: &[Rect]) -> Result<Rect, String> {
    window
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| to_rect(&m))
        .or_else(|| {
            monitors
                .iter()
                .copied()
                .max_by_key(|m| (m.w as i64) * (m.h as i64))
        })
        .ok_or_else(|| "无法确定主屏".to_string())
}

/// 逻辑像素矩形 → 物理像素矩形（拖拽循环手里只有逻辑值，而显示器边界是物理的）。
///
/// 用 `round()` 而不是截断：物理边界差 1px 就可能把「刚好贴边」判成屏外，触发一次无谓回落。
pub fn rect_from_logical(x: f64, y: f64, w: f64, h: f64, scale: f64) -> Rect {
    let s = if scale > 0.0 { scale } else { 1.0 };
    Rect::new(
        (x * s).round() as i32,
        (y * s).round() as i32,
        (w * s).round().max(1.0) as i32,
        (h * s).round().max(1.0) as i32,
    )
}

/// 纯判定：窗口矩形已在屏幕外时，返回应回落到的物理坐标 `Some((x,y))`；在屏内返回 `None`。
///
/// 与 `clamp_to_visible_area` 的区别是**不查询窗口自身** —— 拖拽循环里的位置是自己算出来的，
/// 去查 `outer_position()` 反而可能拿到主线程尚未应用的历史值。
pub fn offscreen_target(
    win: Rect,
    monitors: &[Rect],
    primary: Rect,
    corner: FallbackCorner,
    margin: i32,
) -> Option<(i32, i32)> {
    if rect_visible(win, monitors) {
        None
    } else {
        Some(fallback_position(primary, win, corner, margin))
    }
}

/// 离屏纠偏：窗口已经在屏幕外时，按 `corner` 贴回主屏角落；在屏内则原样返回 `false`。
///
/// 返回 `true` 表示**动了位置**（调用方可据此打日志 / 通知前端把新位置写回持久化，
/// 否则下次重启还会再纠一次）。
pub fn clamp_to_visible_area(
    window: &WebviewWindow,
    corner: FallbackCorner,
    margin: i32,
) -> Result<bool, String> {
    let monitors = monitor_rects(window)?;
    let primary = primary_rect(window, &monitors)?;

    let pos = window
        .outer_position()
        .map_err(|e| format!("读取窗口位置失败: {e}"))?;
    let size = window
        .outer_size()
        .map_err(|e| format!("读取窗口尺寸失败: {e}"))?;
    let win = Rect::new(pos.x, pos.y, size.width as i32, size.height as i32);
    let Some((x, y)) = offscreen_target(win, &monitors, primary, corner, margin) else {
        return Ok(false);
    };
    tracing::warn!(
        saved_x = pos.x,
        saved_y = pos.y,
        corner = ?corner,
        "恢复位置不在任何显示器上，回落到主屏角落"
    );
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("回落设置位置失败: {e}"))?;
    Ok(true)
}

fn to_rect(m: &tauri::Monitor) -> Rect {
    // tauri 2.11 的 `position()` / `size()` 返回**引用**，这里显式解构取字段
    let p = m.position();
    let s = m.size();
    Rect::new(p.x, p.y, s.width as i32, s.height as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单屏 1536×864（v1.2.1 事故机器的真实分辨率）
    const MON: &[Rect] = &[Rect::new(0, 0, 1536, 864)];

    /// 🐞 回归（P1，2026-10-29 @v1.2.12 整改 1.3）：v1.2.1 那次的真实坐标必须判为屏外。
    ///
    /// 副屏拔掉后 localStorage 里留的是负 x（窗口整个在屏幕左侧之外，实测 (-695,159)）。
    /// 用户看到的就是「开关开着但东西不见了」，而旧代码把这份坐标原样恢复。
    #[test]
    fn stale_negative_x_is_out_of_view() {
        let win = Rect::new(-695, 159, 300, 36);
        assert!(
            !rect_visible(win, MON),
            "窗口右边缘 -395 仍在 0 左边，必须判屏外"
        );
        // 桌宠同理：y 为负（副屏在主屏上方时保存的坐标）也要抓到
        assert!(
            !rect_visible(Rect::new(100, -400, 140, 330), MON),
            "整块在屏幕上方的桌宠必须判屏外"
        );
    }

    /// 屏内的三种形态都不能被「纠偏」挪走 —— 否则用户摆好的位置会被随机改掉。
    #[test]
    fn in_view_positions_stay() {
        // 正常左上、贴右下、以及被任务栏挡掉一小半（重叠仍 > 1/3）
        assert!(rect_visible(Rect::new(0, 0, 140, 140), MON));
        assert!(rect_visible(Rect::new(1396, 724, 140, 140), MON));
        assert!(rect_visible(Rect::new(1460, 780, 140, 140), MON));
    }

    /// 判定边界：两轴各 1/3 是**与或**关系 —— 只看水平会漏掉垂直丢失，正是本次补的那一刀。
    #[test]
    fn needs_overlap_on_both_axes() {
        let win = Rect::new(100, 0, 300, 300);
        // 水平充分重叠、垂直只剩 1/3 的边缘：刚好可见（>= 100）
        assert!(
            rect_visible(
                win,
                &[Rect::new(0, 0, 1536, 100), Rect::new(0, 400, 1536, 100)]
            ),
            "垂直重叠恰好 1/3 视为可见（贴边摆放不该被挪走）"
        );
        // 同样的水平重叠，垂直只交 99px（< 300/3）→ 屏外
        assert!(
            !rect_visible(
                Rect::new(100, 0, 300, 300),
                &[Rect::new(0, -201, 1536, 300)]
            ),
            "垂直重叠不足 1/3 必须判屏外：旧实现只看水平，这种位置会漏判"
        );
    }

    /// 多屏：只要落在**任一**块屏上就算可见（副屏 x 为负是常见布局）。
    #[test]
    fn any_monitor_counts() {
        let left = Rect::new(-1920, 0, 1920, 1080);
        let built_in = Rect::new(0, 0, 1536, 864);
        assert!(rect_visible(
            Rect::new(-300, 100, 140, 140),
            &[built_in, left]
        ));
        assert!(!rect_visible(
            Rect::new(-5000, 100, 140, 140),
            &[built_in, left]
        ));
        assert!(
            !rect_visible(Rect::new(100, 100, 140, 140), &[]),
            "一块屏都枚举不到时按屏外处理（调用方会再兜底）"
        );
    }

    /// 回落位：贴着指定角、留 margin，且不会算出负宽高导致跑到更远的屏外。
    #[test]
    fn fallback_lands_inside_the_primary() {
        let mon = Rect::new(0, 0, 1536, 864);
        let win = Rect::new(-695, 159, 300, 36);
        let (x, y) = fallback_position(mon, win, FallbackCorner::TopRight, 24);
        assert_eq!((x, y), (1212, 24));
        assert!(
            rect_visible(win_moved(win, x, y), &[mon]),
            "右上回落必须落在屏内"
        );

        let pet = Rect::new(-4000, -400, 140, 330);
        let (px, py) = fallback_position(mon, pet, FallbackCorner::BottomRight, 24);
        assert_eq!((px, py), (1372, 510));
        assert!(
            rect_visible(win_moved(pet, px, py), &[mon]),
            "右下回落必须落在屏内"
        );

        // 比屏幕还宽的窗口（极端：外接 4K 上保存的位置，回到 1024 宽小屏）
        let (wx, wy) = fallback_position(
            Rect::new(0, 0, 1024, 768),
            Rect::new(5000, 0, 2000, 100),
            FallbackCorner::TopRight,
            24,
        );
        assert_eq!(
            (wx, wy),
            (-1000, 24),
            "窗口比屏还宽时允许负坐标（左上角贴主屏左缘外）"
        );
    }

    fn win_moved(win: Rect, x: i32, y: i32) -> Rect {
        Rect::new(x, y, win.w, win.h)
    }

    /// 逻辑→物理换算：拖拽循环手里是逻辑值、显示器边界是物理值，四舍五入那 1px
    /// 直接决定「刚好贴边」算不算屏外，所以必须 round 不能截断。
    #[test]
    fn logical_to_physical_rounds_instead_of_truncating() {
        // scale=1.25（Windows 常见 125% DPI）
        let r = rect_from_logical(100.4, 50.6, 140.0, 330.0, 1.25);
        assert_eq!((r.x, r.y, r.w, r.h), (126, 63, 175, 413));
        // 异常 scale（0 / 负）按 1.0 处理：宁可判定保守，也不能算出 NaN 坐标把窗口送进黑洞
        let d = rect_from_logical(10.0, 20.0, 140.0, 140.0, 0.0);
        assert_eq!((d.x, d.y, d.w, d.h), (10, 20, 140, 140));
        let n = rect_from_logical(10.0, 20.0, 0.0, 0.0, 1.0);
        assert_eq!(
            (n.w, n.h),
            (1, 1),
            "零尺寸夹紧到 1px，避免除法/重叠算出无意义结果"
        );
    }

    /// 纯判定版：屏内返回 `None`（一帧都不该挪），屏外返回的正是 `fallback_position` 的值。
    #[test]
    fn offscreen_target_only_moves_when_out_of_view() {
        let mon = Rect::new(0, 0, 1536, 864);
        assert_eq!(
            offscreen_target(
                Rect::new(100, 100, 140, 140),
                &[mon],
                mon,
                FallbackCorner::BottomRight,
                64
            ),
            None,
            "屏内绝不能返回回落点"
        );
        assert_eq!(
            offscreen_target(
                Rect::new(-695, 159, 300, 36),
                &[mon],
                mon,
                FallbackCorner::TopRight,
                24
            ),
            Some((1212, 24)),
            "v1.2.1 真实事故坐标必须被抓到并给出右上角回落位"
        );
    }
}
