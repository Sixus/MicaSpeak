//! M3 阶段 A：全局 PTT——WH_KEYBOARD_LL 观测型低级键盘钩子。
//!
//! 设计约束（docs/07 A1/A2/A5）：
//! - 只观察目标虚拟键的 down/up 并驱动发送开关，无条件 CallNextHookEx 放行，
//!   绝不吞键（全局吞掉左 Ctrl 会让用户的 Ctrl+C/Ctrl+S 全部失效）；
//! - 钩子必须装在拥有消息泵的线程上，不装在 cpal 回调或 Tokio 工作线程；
//!   泵线程退出 = 热键静默失效（docs/07 已知坑）；
//! - 回调内只做原子写 + 时间戳，不做 IO、不 emit；日志与发送开关由监视任务完成；
//! - 隐私：非目标键直接放行不记录，目标键只打 VK 码 + down/up + UTC 时间戳；
//!   不过滤合成按键（LLKHF_INJECTED），观测型钩子不区分输入来源。
//!
//! UIPI 边界（不通过日志或重试掩盖）：前台窗口完整性级别更高（管理员权限
//! 运行的程序）时，本进程收不到按键事件，PTT 在该窗口聚焦时不生效。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{error, info};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

/// 当前 PTT 目标虚拟键；0 = 未启用。改绑就是原子替换这个值（旧键立即失效）。
static TARGET_VK: AtomicU32 = AtomicU32::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);

// 钩子回调 → 监视任务的槽。序列号让监视任务看到"有新事件"。
static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);
static EVENT_VK: AtomicU32 = AtomicU32::new(0);
static EVENT_DOWN: AtomicBool = AtomicBool::new(false);
static EVENT_UTC_MS: AtomicU64 = AtomicU64::new(0);

/// 低级键盘钩子回调：只读目标键状态，无条件放行。
unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let vk = info.vkCode;
        let target = TARGET_VK.load(Ordering::Relaxed);
        if target != 0 && vk == target {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            EVENT_VK.store(vk, Ordering::Relaxed);
            EVENT_DOWN.store(down, Ordering::Relaxed);
            EVENT_UTC_MS.store(ms, Ordering::Relaxed);
            EVENT_SEQ.fetch_add(1, Ordering::Release);
        }
    }
    // 观测型：无条件 CallNextHookEx，绝不吞键（docs/07 A1）。
    CallNextHookEx(None, code, wparam, lparam)
}

/// 启动钩子线程。线程必须常驻：低级钩子要求安装线程跑消息泵，
/// 泵退出钩子即静默失效（docs/07 已知坑，M3 验收做反证）。
pub fn spawn_hook_thread() {
    std::thread::Builder::new()
        .name("ptt-hotkey".into())
        .spawn(|| unsafe {
            // LL 钩子不需要 DLL 句柄，传 None。
            match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) {
                Ok(hook) => {
                    INSTALLED.store(true, Ordering::Release);
                    info!("全局 PTT 低级键盘钩子已安装（观测型，不吞键）");
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                    let _ = UnhookWindowsHookEx(hook);
                    INSTALLED.store(false, Ordering::Release);
                    error!("PTT 钩子线程消息泵已退出，全局热键失效");
                }
                Err(e) => {
                    error!("安装 WH_KEYBOARD_LL 钩子失败：{e}（全局 PTT 不可用，设置页会显示状态）")
                }
            }
        })
        .expect("启动 PTT 钩子线程失败");
}

pub fn set_target_vk(vk: u32) {
    TARGET_VK.store(vk, Ordering::Release);
}

pub fn installed() -> bool {
    INSTALLED.load(Ordering::Acquire)
}

/// 监视任务：把回调槽位变成日志与发送开关（docs/07 A2：回调内不做 IO）。
/// 热键与界面按钮是两个独立按住来源：任一按住即发送，都松开才停（A3）。
pub async fn watch_task(audio: crate::audio::AudioManager) {
    let mut last_seq = EVENT_SEQ.load(Ordering::Acquire);
    let mut applied_down = false;
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(20));
    loop {
        interval.tick().await;
        let seq = EVENT_SEQ.load(Ordering::Acquire);
        if seq == last_seq {
            continue;
        }
        last_seq = seq;
        let down = EVENT_DOWN.load(Ordering::Relaxed);
        if down == applied_down {
            // 自动重复产生的连续 down，忽略。
            continue;
        }
        applied_down = down;
        let vk = EVENT_VK.load(Ordering::Relaxed);
        let utc = EVENT_UTC_MS.load(Ordering::Relaxed);
        info!(
            vk,
            event = if down { "down" } else { "up" },
            utc = %format_utc_ms(utc),
            "PTT 热键事件"
        );
        audio.set_key_ptt(down);
    }
}

/// Unix 毫秒 → "YYYY-MM-DD HH:MM:SS.mmm UTC"。
/// 不引入 chrono（依赖白名单只有 nnnoiseless 和 windows）。
pub fn format_utc_ms(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let millis = (ms % 1000) as u32;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (h, m, s) = ((sod / 3600) as u32, ((sod % 3600) / 60) as u32, (sod % 60) as u32);
    // Howard Hinnant civil_from_days：天数 → 公历年月日。
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mth = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if mth <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 };
    format!("{y:04}-{mth:02}-{d:02} {h:02}:{m:02}:{s:02}.{millis:03} UTC")
}
