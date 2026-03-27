use alloc::string::{String};
use alloc::sync::Arc;
use core::fmt::{Display};
use serde::Serialize;

use super::core::read_log;

pub struct LogIterator {
    next_id: usize,
    include_system: bool,
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