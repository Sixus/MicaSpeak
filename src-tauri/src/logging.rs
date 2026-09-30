//! 极简双写日志（M5 A）：stderr（dev/带控制台调试）+ `Data/logs/micaspeak.log`
//! （release 是 windows_subsystem 无控制台 exe，重连退避、冷启动等验收证据
//! 必须落文件）。级别：`MICASPEAK_LOG` 覆盖；release 默认 info，debug 构建默认
//! debug。不引入新依赖：时间戳用标准库自行换算（UTC）。
//!
//! M7a 治理（2026-09-30 10.5GB 日志事故，C 盘被挤爆）：
//! - 运行中也轮转：单文件写入量超 `LOG_ROTATE_BYTES` 就地改名 .old 重开，
//!   磁盘占用上界 ≈ 2×`LOG_ROTATE_BYTES`。此前只在下次启动时轮转，一次挂机
//!   遇上重连风暴就能写出 10GB。
//! - 按库分级：micaspeak 自身按选定级别，第三方库（tsclientlib / hickory /
//!   tauri…）封顶 INFO——hickory 的 DNS DEBUG 一秒能刷几十行；`MICASPEAK_LOG=trace`
//!   是显式全量开关，放开第三方库（排障用，用完记得关）。
//! - 文件关 ANSI 色码（省约 1/3 字节、可直接 grep）；stderr 一直是纯文本。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrd};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::info;

/// 单文件上限：启动时查已有文件、运行中查本次写入量，超限轮转为 .old。
/// 磁盘占用上界 = 当前文件 + .old ≈ 2×该值。
const LOG_ROTATE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct LogSink {
    file: Arc<StdMutex<Option<File>>>,
    path: Arc<PathBuf>,
    written: Arc<AtomicU64>,
}

impl LogSink {
    fn open() -> Option<Self> {
        let dir = crate::persistence::data_root().join("logs");
        let path: PathBuf = dir.join("micaspeak.log");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() > LOG_ROTATE_BYTES {
                rename_to_old(&path);
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(&path).ok()?;
        let sink = LogSink {
            file: Arc::new(StdMutex::new(Some(file))),
            path: Arc::new(path),
            written: Arc::new(AtomicU64::new(0)),
        };
        let (t, _) = utc_parts(SystemTime::now());
        sink.write_line(&format!(
            "===== MicaSpeak 启动 pid={} {t}（UTC）build={} =====",
            std::process::id(),
            if cfg!(debug_assertions) { "debug" } else { "release" }
        ));
        Some(sink)
    }

    fn write_line(&self, line: &str) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = writeln!(file, "{line}");
                self.written.fetch_add(line.len() as u64 + 1, AtomicOrd::Relaxed);
            }
        }
    }

    /// 写入量超限就就地轮转：关句柄 → 改名 .old → 重开。Windows 下打开中的
    /// 文件不能改名，必须先取走句柄；整个过程持文件锁，并发写线程要么写进
    /// 旧文件、要么等轮转完成后写进新文件，不会交错。
    fn rotate_if_needed(&self) {
        if self.written.load(AtomicOrd::Relaxed) < LOG_ROTATE_BYTES {
            return;
        }
        if let Ok(mut guard) = self.file.lock() {
            // 双检：抢锁前别的写线程可能刚轮转完并把计数清零。
            if self.written.load(AtomicOrd::Relaxed) < LOG_ROTATE_BYTES {
                return;
            }
            *guard = None;
            rename_to_old(&self.path);
            *guard = OpenOptions::new().create(true).append(true).open(&*self.path).ok();
            self.written.store(0, AtomicOrd::Relaxed);
        }
    }
}

/// 把当前日志改名 .old（Windows 的 rename 语义会覆盖已存在的 .old）。失败
/// （极少）静默，留给下次轮转再试。
fn rename_to_old(path: &Path) {
    let old = path.with_file_name("micaspeak.log.old");
    let _ = fs::rename(path, old);
}

/// tracing_subscriber 的 MakeWriter：tracing 事件写文件。
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogSink {
    type Writer = SinkWriter<'a>;
    fn make_writer(&'a self) -> Self::Writer {
        SinkWriter(self)
    }
}

pub struct SinkWriter<'a>(&'a LogSink);

impl std::io::Write for SinkWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.rotate_if_needed();
        if let Ok(mut guard) = self.0.file.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = file.write(buf);
                self.0.written.fetch_add(buf.len() as u64, AtomicOrd::Relaxed);
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if let Ok(mut guard) = self.0.file.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = file.flush();
            }
        }
        Ok(())
    }
}

/// log crate 适配：同一行写文件 + stderr（stderr 失败静默，dev 才有控制台）。
/// M7a：按 target 分级——micaspeak 自身用 own 级别，第三方库用封顶级别。
struct DualLogger {
    sink: LogSink,
    own: log::LevelFilter,
    third: log::LevelFilter,
}

impl log::Log for DualLogger {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        meta.level() <= level_for_target(meta.target(), self.own, self.third)
    }
    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let (t, _) = utc_parts(SystemTime::now());
        let line = format!("[{t} {:>5}] {}", record.level(), record.args());
        self.sink.write_line(&line);
        eprintln!("{line}");
    }
    fn flush(&self) {}
}

/// M7a：按 target 前缀分级——`micaspeak` 前缀是本项目自己的日志，用自身
/// 级别；其余 target 全是依赖库，用封顶级别。
fn level_for_target(
    target: &str,
    own: log::LevelFilter,
    third: log::LevelFilter,
) -> log::LevelFilter {
    if target.starts_with("micaspeak") {
        own
    } else {
        third
    }
}

/// (HH:MM:SS, 完整日期串)。UTC；日志时间线足够（文件头记录启动日期）。
pub fn utc_parts(t: SystemTime) -> (String, String) {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let secs = d.as_secs();
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant civil_from_days：epoch(1970-01-01) → (y, m, d)。
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (
        format!("{h:02}:{m:02}:{s:02}"),
        format!("{year:04}-{month:02}-{day:02}"),
    )
}

static START: OnceLock<Instant> = OnceLock::new();
static READY_LOGGED: StdMutex<bool> = StdMutex::new(false);

/// 进程入口打点（main 第一行调用）。
pub fn mark_startup() {
    let _ = START.set(Instant::now());
}

/// 首屏数据就绪打点（首次 get_app_snapshot 调用），只记一次。
pub fn log_first_frame_ready() {
    {
        let mut logged = READY_LOGGED.lock().unwrap();
        if *logged {
            return;
        }
        *logged = true;
    }
    if let Some(t0) = START.get() {
        info!("冷启动计时：进程启动 → 首屏数据就绪 {} ms", t0.elapsed().as_millis());
    }
}

/// 初始化两级日志。返回 LogSink 供 tracing_subscriber 复用。
pub fn init() {
    mark_startup();
    let default = if cfg!(debug_assertions) {
        tracing_subscriber::filter::LevelFilter::DEBUG
    } else {
        tracing_subscriber::filter::LevelFilter::INFO
    };
    let (tmax, lmax) = match std::env::var("MICASPEAK_LOG")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "trace" => (
            tracing_subscriber::filter::LevelFilter::TRACE,
            log::LevelFilter::Trace,
        ),
        "debug" => (
            tracing_subscriber::filter::LevelFilter::DEBUG,
            log::LevelFilter::Debug,
        ),
        "warn" => (
            tracing_subscriber::filter::LevelFilter::WARN,
            log::LevelFilter::Warn,
        ),
        "error" => (
            tracing_subscriber::filter::LevelFilter::ERROR,
            log::LevelFilter::Error,
        ),
        "info" => (
            tracing_subscriber::filter::LevelFilter::INFO,
            log::LevelFilter::Info,
        ),
        _ => (default, level_filter_to_log(default)),
    };
    // M7a 按库分级：micaspeak 自身 tmax；第三方封顶 INFO（更低级别照旧），
    // 仅 MICASPEAK_LOG=trace 显式放开到全量。
    let third = if tmax == tracing_subscriber::filter::LevelFilter::TRACE {
        tracing_subscriber::filter::LevelFilter::TRACE
    } else if tmax > tracing_subscriber::filter::LevelFilter::INFO {
        tracing_subscriber::filter::LevelFilter::INFO
    } else {
        tmax
    };
    if let Some(sink) = LogSink::open() {
        // Targets 依赖的 registry feature 已由 fmt 连带启用，无需动 Cargo.toml。
        let targets = tracing_subscriber::filter::Targets::new()
            .with_default(third)
            .with_target("micaspeak", tmax);
        use tracing_subscriber::prelude::*;
        use tracing_subscriber::util::SubscriberInitExt;
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(sink.clone()))
            .with(targets)
            .init();
        let third_log = level_filter_to_log(third);
        log::set_boxed_logger(Box::new(DualLogger { sink, own: lmax, third: third_log }))
            .ok();
        // 全局上限取两者较大值；具体放行由 level_for_target 分级决定。
        log::set_max_level(std::cmp::max(lmax, third_log));
    }
}

fn level_filter_to_log(f: tracing_subscriber::filter::LevelFilter) -> log::LevelFilter {
    match f {
        tracing_subscriber::filter::LevelFilter::TRACE => log::LevelFilter::Trace,
        tracing_subscriber::filter::LevelFilter::DEBUG => log::LevelFilter::Debug,
        tracing_subscriber::filter::LevelFilter::INFO => log::LevelFilter::Info,
        tracing_subscriber::filter::LevelFilter::WARN => log::LevelFilter::Warn,
        _ => log::LevelFilter::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M7a：第三方库 DEBUG（hickory DNS 风暴元凶）被封顶 INFO，micaspeak
    /// 自身在 debug 构建放开到 DEBUG；WARN 对任何库都放行。
    #[test]
    fn third_party_debug_is_capped() {
        let (own, third) = (log::LevelFilter::Debug, log::LevelFilter::Info);
        assert!(log::Level::Debug <= level_for_target("micaspeak::conn", own, third));
        assert!(log::Level::Debug > level_for_target("hickory_resolver::xfer", own, third));
        assert!(log::Level::Debug > level_for_target("tsclientlib::resolver", own, third));
        assert!(log::Level::Info <= level_for_target("tsclientlib", own, third));
        assert!(log::Level::Warn <= level_for_target("tauri", own, third));
    }

    /// M7a：release 默认 INFO 时，第三方与自身同级，只剩 ERROR 以下差异。
    #[test]
    fn release_default_is_info_everywhere() {
        let (own, third) = (log::LevelFilter::Info, log::LevelFilter::Info);
        assert!(log::Level::Info <= level_for_target("micaspeak::audio", own, third));
        assert!(log::Level::Debug > level_for_target("hickory_net::xfer", own, third));
    }
}
