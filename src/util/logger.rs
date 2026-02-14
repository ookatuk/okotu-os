use alloc::string::ToString;
use core::panic::Location;
#[macro_export]
macro_rules! log_custom {
    ($level:expr,$by:expr,$tag:expr,$($text:tt)*) => { $crate::util::logger::_custom($level, $by, $tag, format_args!($($text)*)) };
}

#[macro_export]
macro_rules! log_trace {
    ($by:expr,$tag:expr,$($text:tt)*) => { if cfg!(feature = "debug-mode") {$crate::log_custom!("trace", $by, $tag, $($text)*)} };
}

#[macro_export]
macro_rules! log_debug {
    ($by:expr,$tag:expr,$($text:tt)*) => { $crate::log_custom!("debug", $by, $tag, $($text)*) };
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

#[track_caller]
pub fn _custom(level: &str, by: &str, tag: &str, text: core::fmt::Arguments) {
    let location = Location::caller();
    text.to_string().split('\n').for_each(|line| _custom_internal(level, by, tag, line, location));
}

pub struct OsLog<'a> {
    pub file: &'a str,
    pub line: u32,
    pub column: u32,
    pub level: &'a str,  // LogLevel (カスタム文字列)
    pub by: &'a str,  // by (コンポーネント名)
    pub tag: &'a str,  // tag (サブカテゴリ)
    pub data: &'a str,  // data/text (メッセージ本体)
}

#[track_caller]
pub fn _custom_internal(level: &str, by: &str, tag: &str, text: &str, loc: &Location) {
    let _data = OsLog {
        level,
        by,
        tag,
        data: text,

        file: loc.file(),
        line: loc.line(),
        column: loc.column(),
    };
}
