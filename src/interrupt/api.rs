use x86_64::structures::gdt::SegmentSelector;
use x86_64::structures::idt::InterruptStackFrame;
use x86_64::{PrivilegeLevel, VirtAddr};
use crate::result;
use crate::result::{Error, ErrorType};
use crate::thread_local::read_gs;

pub fn init() {
    super::raw::init();
}

pub fn add(id: u8, target: extern "x86-interrupt" fn(InterruptStackFrame), over_write: bool) -> result::Result<&'static mut x86_64::structures::idt::EntryOptions> {
    let gs = read_gs().unwrap();

    if !over_write && !gs.idt_raw[id].handler_addr().is_null() {
        return Error::new(
            ErrorType::AlreadyInitialized,
            Some("this idt vector is initialized. if you need over write, enable over_write flag.")
        ).raise();
    }

    Ok(unsafe{gs.idt_raw[id].set_handler_fn(target).set_present(true).set_code_selector(SegmentSelector::new(
        1,
        PrivilegeLevel::Ring0
    ))})
}

pub fn remove(id: u8) {
    let gs = read_gs().unwrap();
    unsafe{gs.idt_raw[id].set_handler_addr(VirtAddr::zero()).set_present(false)};
}