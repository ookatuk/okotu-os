use x86_64::structures::idt::InterruptStackFrame;
use crate::cpu::msr;
use crate::{cpu_info, log_error};

const IA32_APIC_BASE_MSR: u32 = 0x1B;
const X2APIC_MSR_BASE: u32 = 0x800;
const X2APIC_MSR_ICR: u32 = 0x830;
const X2APIC_ESR_MSR: u32 = 0x828;

const APIC_REG_TPR: u32 = 0x80;
const APIC_REG_EOI: u32 = 0xB0;
const APIC_REG_SVR: u32 = 0xF0;
const APIC_REG_ESR: u32 = 0x280;
const APIC_REG_ICR_LOW: u32 = 0x300;
const APIC_REG_ICR_HIGH: u32 = 0x310;
const LVT_REGS: [u32; 6] = [0x320, 0x330, 0x340, 0x350, 0x360, 0x370];

const ICR_FIXED: u64 = 0x000 << 8;
const ICR_INIT: u64 = 0x101 << 8;
const ICR_STARTUP: u64 = 0x110 << 8;
const ICR_ASSERT: u64 = 1 << 14;

const XAPIC_BASE_ADDR: u64 = 0xFEE00000;

pub const SPURIOUS_VECTOR: u8 = 0xFF;
pub const ERROR_VECTOR: u8 = 0xFE;

unsafe fn is_x2apic_active() -> bool {
    let base = unsafe{msr::read(IA32_APIC_BASE_MSR)};
    (base & (1 << 10)) != 0
}

unsafe fn write_apic(reg_offset: u32, value: u32) {unsafe{
    if is_x2apic_active() {
        msr::write(X2APIC_MSR_BASE + (reg_offset >> 4), value as u64);
    } else {
        let addr = XAPIC_BASE_ADDR + reg_offset as u64;
        core::ptr::write_volatile(addr as *mut u32, value);
    }
}}

unsafe fn read_apic(reg_offset: u32) -> u32 {unsafe{
    if is_x2apic_active() {
        msr::read(X2APIC_MSR_BASE + (reg_offset >> 4)) as u32
    } else {
        let addr = XAPIC_BASE_ADDR + reg_offset as u64;
        core::ptr::read_volatile(addr as *const u32)
    }
}}

unsafe fn read_esr() -> u32 {unsafe{
    if is_x2apic_active() {
        msr::write(X2APIC_ESR_MSR, 0);
        msr::read(X2APIC_ESR_MSR) as u32
    } else {
        let esr_ptr = (XAPIC_BASE_ADDR + APIC_REG_ESR as u64) as *mut u32;
        core::ptr::write_volatile(esr_ptr, 0); // ダミー書き込み
        core::ptr::read_volatile(esr_ptr)
    }
}}

fn log_apic_error(esr: u32) {
    if esr == 0 { return; }
    if esr & (1 << 7) != 0 { log_error!("kernel", "apic", "Illegal Vector (Send)"); }
    if esr & (1 << 6) != 0 { log_error!("kernel", "apic", "Illegal Vector (Receive)"); }
    if esr & (1 << 5) != 0 { log_error!("kernel", "apic", "Send Illegal Vector"); }
    if esr & (1 << 3) != 0 { log_error!("kernel", "apic", "Receive Accept Error"); }
    if esr & (1 << 2) != 0 { log_error!("kernel", "apic", "Send Accept Error"); }
}

pub unsafe fn send_eoi() {unsafe{
    write_apic(APIC_REG_EOI, 0);
}}

pub unsafe fn send_fixed_ipi(apic_id: u32, vector: u8) {unsafe{
    let cmd = ICR_FIXED | ICR_ASSERT | (vector as u64);
    if is_x2apic_active() {
        let icr = ((apic_id as u64) << 32) | cmd;
        msr::write(X2APIC_MSR_ICR, icr);
    } else {
        write_apic(APIC_REG_ICR_HIGH, apic_id << 24);
        write_apic(APIC_REG_ICR_LOW, cmd as u32);
    }
}}

pub unsafe fn send_init_ipi(apic_id: u32) {unsafe{
    let cmd = ICR_INIT | ICR_ASSERT;
    if is_x2apic_active() {
        msr::write(X2APIC_MSR_ICR, ((apic_id as u64) << 32) | cmd);
    } else {
        write_apic(APIC_REG_ICR_HIGH, apic_id << 24);
        write_apic(APIC_REG_ICR_LOW, cmd as u32);
    }
}}

pub unsafe fn send_sipi(apic_id: u32, vector: u8) {unsafe{
    let cmd = ICR_STARTUP | (vector as u64);
    if is_x2apic_active() {
        msr::write(X2APIC_MSR_ICR, ((apic_id as u64) << 32) | cmd);
    } else {
        write_apic(APIC_REG_ICR_HIGH, apic_id << 24);
        write_apic(APIC_REG_ICR_LOW, cmd as u32);
    }
}}

extern "x86-interrupt" fn spurious_handler(_stack_frame: InterruptStackFrame) {}

extern "x86-interrupt" fn error_handler(_stack_frame: InterruptStackFrame) {
    let esr = unsafe { read_esr() };
    log_apic_error(esr);
    unsafe { send_eoi(); }
}

pub fn init_local_apic() {
    unsafe {
        let mut base_msr = msr::read(IA32_APIC_BASE_MSR);
        base_msr |= 1 << 11;
        if cpu_info!(environment::apic::X2Supported) {
            base_msr |= 1 << 10;
        }
        msr::write(IA32_APIC_BASE_MSR, base_msr);

        for &reg in &LVT_REGS {
            let val = read_apic(reg);
            write_apic(reg, val | (1 << 16));
        }

        write_apic(APIC_REG_TPR, 0);
        write_apic(APIC_REG_SVR, (1 << 8) | (SPURIOUS_VECTOR as u32));
        write_apic(0x370, ERROR_VECTOR as u32);
    }

    crate::interrupt::api::add(SPURIOUS_VECTOR, spurious_handler, true).unwrap().set_present(true);
    crate::interrupt::api::add(ERROR_VECTOR, error_handler, true).unwrap().set_present(true);
}
