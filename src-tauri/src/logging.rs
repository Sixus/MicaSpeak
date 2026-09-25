//! 极简双写日志（M5 A）：stderr（dev/带控制台调试）+ `Data/logs/micaspeak.log`
//! （release 是 windows_subsystem 无控制台 exe，重连退避、冷启动等验收证据
//! 必须落文件）。级别：`MICASPEAK_LOG` 覆盖；release 默认 info，debug 构建默认
//! debug。不引入新依赖：时间戳用标准库自行换算（UTC）。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::info;

/// 文件超过该大小则在下次启动时轮转为 .old（运行中不做轮转，量级可控）。
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone)]
pub struct LogSink {
    file: Arc<StdMutex<Option<File>>>,
}

impl LogSink {
    fn open() -> Option<Self> {
        let dir = crate::persistence::data_root().join("logs");
        let path: PathBuf = dir.join("micaspeak.log");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() > MAX_LOG_BYTES {
                let _ = fs::rename(&path, dir.join("micaspeak.log.old"));
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(&path).ok()?;
        let sink = LogSink { file: Arc::new(StdMutex::new(Some(file))) };
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
            }
        }
    }
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
        if let Ok(mut guard) = self.0.file.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = file.write(buf);
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
struct DualLogger {
    sink: LogSink,
    level: log::LevelFilter,
}

impl log::Log for DualLogger {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        meta.level() <= self.level
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
    if let Some(sink) = LogSink::open() {
        tracing_subscriber::fmt().with_max_level(tmax).with_writer(sink.clone()).init();
        log::set_boxed_logger(Box::new(DualLogger { sink, level: lmax }))
            .ok();
        log::set_max_level(lmax);
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
