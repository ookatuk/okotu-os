
#[macro_export]
macro_rules! log_custom {
    ($level:expr,$by:expr,$tag:expr,$($text:tt)*) => { $crate::logger::utils::_custom($level, $by, $tag, format_args!($($text)*)) };
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
