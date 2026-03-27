#[cfg(feature = "enable_uart_outputs")]
use crate::io::console::serial::SERIAL1;

use alloc::collections::VecDeque;
use alloc::string::ToString;
use alloc::sync::Arc;
use core::ops::DerefMut;
use core::panic::Location;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::{Lazy, RwLock};
use x86_64::instructions::interrupts;

use super::types::OsLog;
use super::utils::UartTmp;

pub static LOG_CAPACITY: AtomicUsize = AtomicUsize::new(5000);

pub(crate) static LOG_BUF: Lazy<RwLock<VecDeque<Arc<OsLog>>>> =
    Lazy::new(|| RwLock::new(VecDeque::with_capacity(LOG_CAPACITY.load(Ordering::SeqCst))));

pub(crate) static LOG_HEAD_ID: AtomicUsize = AtomicUsize::new(0);


pub(crate) fn custom_internal(
    level: &'static str,
    by: &'static str,
    tag: &'static str,
    text: core::fmt::Arguments,
    loc: &'static Location,
) {
    let time = 0;

    let data = OsLog {
        time,
        level,
        by,
        tag,
        data: text.to_string(),

        file: loc.file(),
        line: loc.line(),
        column: loc.column(),

        cpu_acpi_id: 0,
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


#[inline]
pub fn get_log_min_id() -> usize {
    LOG_HEAD_ID.load(Ordering::SeqCst)
}