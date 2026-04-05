//! local apic helper
//! Since it's close to raw,
//! we recommend using [`crate::multi_core::api`].
//!
//! # Safety
//! Incorrect order = `#GP` and other...

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
const ICR_INIT: u64 = 0b101 << 8;
pub const ICR_STARTUP: u64 = 0b110 << 8;
const ICR_ASSERT: u64 = 1 << 14;

const XAPIC_BASE_ADDR: u64 = 0xFEE00000;

pub const SPURIOUS_VECTOR: u8 = 0xFF;
pub const ERROR_VECTOR: u8 = 0xFE;

const ICR_DEST_ALL_EXC_SELF: u64 = 0b11 << 18;
const ICR_LEVEL_ASSERT: u64 = 1 << 14;

/// return x2apic is supported.
#[inline]
fn is_x2apic_active() -> bool {
    let base = unsafe{msr::read(IA32_APIC_BASE_MSR)};
    (base & (1 << 10)) != 0
}

/// write to apic
/// # Safety
/// 1. Incorrect order = cpu of death
/// 2. Since the system doesn't check if the input was successful,
///    there's a possibility that the data might be overwritten.
/// 3. In x2apic mode, the upper 32 bits of the input offset are ignored.
/// # Side Effects
/// 1. Writing certain values may have an immediate impact.
unsafe fn write_apic(reg_offset: u32, value: u32) {
    unsafe {
        if is_x2apic_active() {
            if reg_offset == APIC_REG_ICR_HIGH {
                return;
            }
            msr::write(X2APIC_MSR_BASE + (reg_offset >> 4), value as u64);
        } else {
            let addr = XAPIC_BASE_ADDR + reg_offset as u64;
            core::ptr::write_volatile(addr as *mut u32, value);
        }
    }
}

/// read to apic
/// # Safety
/// 1. This func does not check for any ongoing data writes or other issues during the reading process.
unsafe fn read_apic(reg_offset: u32) -> u32 {
    if is_x2apic_active() {
        if reg_offset == APIC_REG_ICR_HIGH {
            return 0;
        }
        unsafe{msr::read(X2APIC_MSR_BASE + (reg_offset >> 4)) as u32}
    } else {
        let addr = XAPIC_BASE_ADDR + reg_offset as u64;
        unsafe{core::ptr::read_volatile(addr as *const u32)}
    }
}

/// return esr.
/// # Safety
/// 1. Please ensure that lapic is enabled.
unsafe fn read_esr() -> u32 {
    unsafe {
        if is_x2apic_active() {
            msr::write(X2APIC_ESR_MSR, 0);
            msr::read(X2APIC_ESR_MSR) as u32
        } else {
            let esr_ptr = (XAPIC_BASE_ADDR + APIC_REG_ESR as u64) as *mut u32;
            core::ptr::write_volatile(esr_ptr, 0);
            core::ptr::read_volatile(esr_ptr)
        }
    }
}

/// send apic error description
fn log_apic_error(esr: u32) {
    if esr == 0 { return; }
    if esr & (1 << 7) != 0 { log_error!("kernel", "apic", "Illegal Vector (Send)"); }
    if esr & (1 << 6) != 0 { log_error!("kernel", "apic", "Illegal Vector (Receive)"); }
    if esr & (1 << 5) != 0 { log_error!("kernel", "apic", "Send Illegal Vector"); }
    if esr & (1 << 3) != 0 { log_error!("kernel", "apic", "Receive Accept Error"); }
    if esr & (1 << 2) != 0 { log_error!("kernel", "apic", "Send Accept Error"); }
}

/// send `eoi`
/// # Safety
/// 1. Do not use `eoi` for certain interrupts.
/// 2. Do not run this outside of an interrupt.
#[inline]
pub unsafe fn send_eoi() {
    unsafe{write_apic(APIC_REG_EOI, 0)};
}

/// Sends a Fixed IPI to a specific target CPU.
///
/// # Safety
/// * The caller must ensure `apic_id` is valid and the target CPU is ready to receive interrupts.
/// * In xAPIC mode, this involves two separate writes to the ICR. This operation is not atomic.
pub unsafe fn send_fixed_ipi(apic_id: u32, vector: u8) {
    let cmd = ICR_FIXED | ICR_ASSERT | (vector as u64);
    if is_x2apic_active() {
        let icr_value = ((apic_id as u64) << 32) | cmd;
        msr::write(X2APIC_MSR_ICR, icr_value);
    } else {
        write_apic(APIC_REG_ICR_HIGH, apic_id << 24);
        write_apic(APIC_REG_ICR_LOW, cmd as u32);
    }
}

/// Sends an INIT IPI to the target CPU to reset it.
///
/// # Safety
/// * This should only be used during the AP (Application Processor) boot sequence.
/// * Sending an INIT IPI to the BSP or an already running CPU can cause a system crash.
pub unsafe fn send_init_ipi(apic_id: u32) {
    let cmd = ICR_INIT | ICR_ASSERT;
    if is_x2apic_active() {
        msr::write(X2APIC_MSR_ICR, ((apic_id as u64) << 32) | cmd);
    } else {
        write_apic(APIC_REG_ICR_HIGH, apic_id << 24);
        write_apic(APIC_REG_ICR_LOW, cmd as u32);
    }
}

/// Sends a Startup IPI (SIPI) to the target CPU.
///
/// # Safety
/// * The `vector` defines the address where the target CPU starts execution (0xVV000).
/// * This must be sent after a successful INIT IPI sequence.
pub unsafe fn send_sipi(apic_id: u32, vector: u8) {
    let cmd = ICR_STARTUP | (vector as u64);
    if is_x2apic_active() {
        msr::write(X2APIC_MSR_ICR, ((apic_id as u64) << 32) | cmd);
    } else {
        write_apic(APIC_REG_ICR_HIGH, apic_id << 24);
        write_apic(APIC_REG_ICR_LOW, cmd as u32);
    }
}

extern "x86-interrupt" fn spurious_handler(_stack_frame: InterruptStackFrame) {
    return;
}

extern "x86-interrupt" fn error_handler(_stack_frame: InterruptStackFrame) {
    let esr = unsafe { read_esr() };
    log_apic_error(esr);
    unsafe { send_eoi(); }
}

/// Initializes the Local APIC on the current CPU.
///
/// This function handles the transition to x2APIC mode (if supported),
/// configures the Spurious and Error vectors, and masks all LVT entries.
///
/// # Safety
/// * This must be called once per CPU core during the initialization phase.
/// * The caller must ensure that the GDT and IDT are properly configured
///   before calling this, as it registers interrupt handlers.
pub fn init_local_apic() {
    unsafe {
        let mut base_msr = msr::read(IA32_APIC_BASE_MSR);
        base_msr |= 1 << 11;
        if cpu_info!(environment::apic::X2Supported) {
            base_msr |= 1 << 10;
        }
        msr::write(IA32_APIC_BASE_MSR, base_msr);

        write_apic(APIC_REG_ESR, 0);
        write_apic(APIC_REG_ESR, 0);

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

/// Broadcasts an IPI to all CPUs excluding the current one.
///
/// # Safety
/// * The caller must ensure that the `mode_flags` are appropriate for a broadcast.
/// * In xAPIC mode, this function waits (spins) for the Delivery Status bit to clear.
pub unsafe fn broadcast_init_ipi_exc_self() {
    let delivery_mode = ICR_INIT | ICR_LEVEL_ASSERT;

    broadcast_ipi_exc_self(delivery_mode, 0);
}

/// Broadcasts an IPI to all CPUs excluding the current one.
///
/// # Safety
/// * The caller must ensure that the `mode_flags` are appropriate for a broadcast.
/// * In xAPIC mode, this function waits (spins) for the Delivery Status bit to clear.
pub unsafe fn broadcast_ipi_exc_self(mode_flags: u64, vector: u8) {
    let cmd = ICR_DEST_ALL_EXC_SELF | mode_flags | (vector as u64);

    if is_x2apic_active() {
        msr::write(X2APIC_MSR_ICR, cmd);
    } else {
        write_apic(APIC_REG_ICR_HIGH, 0);
        write_apic(APIC_REG_ICR_LOW, cmd as u32);

        while (read_apic(APIC_REG_ICR_LOW) & (1 << 12)) != 0 {
            core::hint::spin_loop();
        }
    }
}