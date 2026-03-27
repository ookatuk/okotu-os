use spin::Lazy;

pub mod types;
pub mod core;
pub mod rhai;
pub mod io;
pub mod seride;

pub mod items {
    #![allow(unused_imports)]

    use super::types::*;
}

pub static OS_VERSION: Lazy<types::VersionInfo> = Lazy::new(|| {
    types::VersionInfo::new_os()
});