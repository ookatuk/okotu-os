#[cfg(feature = "enable_uart_outputs")]
use crate::io::console::serial::SERIAL1;
use crate::util::timer::TSC;
use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use bincode::enc::write::Writer;
use bincode::error::EncodeError;
use core::fmt::{Display, Write};
use core::ops::{Deref, DerefMut};
use core::panic::Location;
use core::sync::atomic::{AtomicUsize, Ordering};
use serde::Serialize;
use spin::{Lazy, RwLock};
use uart_16550::SerialPort;
use x86_64::instructions::interrupts;

pub static LOG_CAPACITY: AtomicUsize = AtomicUsize::new(5000);

pub(crate) static LOG_BUF: Lazy<RwLock<VecDeque<Arc<OsLog>>>> =
    Lazy::new(|| RwLock::new(VecDeque::with_capacity(LOG_CAPACITY.load(Ordering::SeqCst))));

static LOG_HEAD_ID: AtomicUsize = AtomicUsize::new(0); // 0番目の要素の通算ID

pub fn add_log(data: &OsLog) {
    let log = Arc::new(data.clone());

    interrupts::without_interrupts(|| {
        let mut lock = LOG_BUF.write();

        let cap = LOG_CAPACITY.load(Ordering::SeqCst);

        if cap == 0 {
            return;
        }

        if lock.len() == cap {
            lock.pop_front();
            LOG_HEAD_ID.fetch_add(1, Ordering::SeqCst);
        }

        lock.push_back(log);
    })
}

pub fn read_log(target_id: usize) -> Option<Arc<OsLog>> {
    interrupts::without_interrupts(|| {
        let head = LOG_HEAD_ID.load(Ordering::SeqCst);
        let lock = LOG_BUF.read();
        let current_len = lock.len();

        if target_id < head {
            return None;
        }

        if target_id >= head + current_len {
            return None;
        }

        let index = target_id - head;
        lock.get(index).cloned()
    })
}

pub struct LogIterator {
    next_id: usize,
    include_system: bool, // "s" を含むかどうか
}

impl LogIterator {
    pub const fn new(start_id: usize, include_system: bool) -> Self {
        Self {
            next_id: start_id,
            include_system,
        }
    }
}

impl Iterator for LogIterator {
    type Item = Arc<OsLog>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let log = read_log(self.next_id)?;
            self.next_id += 1;

            let is_system = log.level == "s";

            if self.include_system == is_system {
                return Some(log);
            }
        }
    }
}

#[inline]
pub fn get_log_min_id() -> usize {
    LOG_HEAD_ID.load(Ordering::SeqCst)
}

#[macro_export]
macro_rules! log_custom {
    ($level:expr,$by:expr,$tag:expr,$($text:tt)*) => { $crate::util::logger::_custom($level, $by, $tag, format_args!($($text)*)) };
}

#[macro_export]
macro_rules! log_trace {
    ($by:expr,$tag:expr,$($text:tt)*) => { if cfg!(feature = "enable_debug_level_outputs") {$crate::log_custom!("trace", $by, $tag, $($text)*)} };
}

#[macro_export]
macro_rules! log_debug {
    ($by:expr,$tag:expr,$($text:tt)*) => { if cfg!(feature = "enable_debug_level_outputs") {$crate::log_custom!("debug", $by, $tag, $($text)*)} };
}

#[macro_export]
macro_rules! log_info {
    ($by:expr,$tag:expr,$($text:tt)*) => { $crate::log_custom!("info", $by, $tag, $($text)*) };
}

#[macro_export]
macro_rules! log_warn {
    ($by:expr,$tag:expr,$($text:tt)*) => { $crate::log_custom!("warn", $by, $tag, $($text)*) };
}

#[macro_export]
macro_rules! log_error {
    ($by:expr,$tag:expr,$($text:tt)*) => { $crate::log_custom!("error", $by, $tag, $($text)*) };
}

#[macro_export]
macro_rules! log_last {
    ($by:expr,$tag:expr,$($text:tt)*) => { $crate::log_custom!("last", $by, $tag, $($text)*) };
}

#[macro_export]
macro_rules! deb {
    ($fmt:expr $(, $arg:tt)*) => { $crate::log_debug!("kernel", "debug", $fmt $(, $arg)*) };
}

#[track_caller]
pub fn _custom(
    level: &'static str,
    by: &'static str,
    tag: &'static str,
    text: core::fmt::Arguments,
) {
    let location = Location::caller();
    custom_internal(level, by, tag, text, location);
}

#[derive(Serialize, Clone)]
#[repr(C)]
pub struct OsLog {
    pub level: &'static str,
    pub by: &'static str,
    pub tag: &'static str,
    pub data: String,
    pub file: &'static str,
    pub time: u64,
    pub line: u32,
    pub column: u32,
    pub cpu_acpi_id: u32,
}

impl Display for OsLog {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "({}), [{:<5}] [{:<5}] {} (at {}:{}:{})",
            self.cpu_acpi_id, self.level, self.tag, self.data, self.file, self.line, self.column,
        )
    }
}

impl OsLog {
    pub fn to_short_string(&self) -> String {
        alloc::format!(
            "({}) [{}] {}: {}",
            self.cpu_acpi_id,
            self.level,
            self.tag,
            self.data
        )
    }
}

#[inline(always)]
pub fn custom_internal(
    level: &'static str,
    by: &'static str,
    tag: &'static str,
    text: core::fmt::Arguments,
    loc: &'static Location,
) {
    _real_custom_internal(level, by, tag, text, loc);
}

pub fn _real_custom_internal(
    level: &'static str,
    by: &'static str,
    tag: &'static str,
    text: core::fmt::Arguments,
    loc: &'static Location,
) {
    let mut time = 0;

    unsafe {
        let tsc = TSC.read();
        if let Some(clock) = tsc.clock_in_100ms.as_ref() {
            let tsc_per_ms = clock.get() / 100;

            time = tsc.now_clock() / tsc_per_ms;
        }
    }

    let data = OsLog {
        time,
        level,
        by,
        tag,
        data: text.to_string(),

        file: loc.file(),
        line: loc.line(),
        column: loc.column(),

        cpu_acpi_id: crate::cpu::utils::who_am_i(),
    };

    add_log(&data);

    #[cfg(feature = "enable_uart_outputs")]
    {
        interrupts::without_interrupts(|| {
            let mut lk_lock = SERIAL1.lock();
            let mut lk = UartTmp(lk_lock.deref_mut());
            lk.send_raw(0xAA);
            lk.send_raw(0xBB);
            lk.send_raw(0xCC);
            lk.send_raw(0xEE);

            let _ = bincode::serde::encode_into_writer(data, lk, bincode::config::standard());
        })
    }
}

struct UartTmp<'a>(&'a mut SerialPort);

impl Writer for UartTmp<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        for i in bytes.iter() {
            self.0.send_raw(*i);
        }

        Ok(())
    }
}

impl Deref for UartTmp<'_> {
    type Target = SerialPort;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for UartTmp<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
