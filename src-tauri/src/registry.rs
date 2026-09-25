//! 直接读注册表（零进程派生）。
//!
//! 背景（M5c 指标测量发现）：GUI 子系统（release）进程派生 reg.exe 这类
//! 控制台程序时，Windows 要为其新建控制台；在云桌面/RDP 会话上实测每次
//! 可耗 3-6 秒，是冷启动杀手。改为 RegGetValueW 直读，微秒级且无副作用。

#[cfg(windows)]
pub fn read_string(hkey_root: windows::Win32::System::Registry::HKEY, subkey: &str, value: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};

    let subkey_w: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let value_w: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let mut buf = [0u16; 512];
    let mut len = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            hkey_root,
            PCWSTR(subkey_w.as_ptr()),
            PCWSTR(value_w.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    };
    if rc.is_err() {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let text = String::from_utf16_lossy(&buf[..end]);
    let trimmed = text.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

#[cfg(not(windows))]
pub fn read_string<H, S: Into<String>, V: Into<String>>(_h: H, _s: S, _v: V) -> Option<String> {
    None
}
