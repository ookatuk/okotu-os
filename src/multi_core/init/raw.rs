use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ops::Index;
use x86_64::structures::DescriptorTablePointer;
use x86_64::VirtAddr;
use x86_64::structures::gdt::GlobalDescriptorTable;
use crate::{cpu_info, result, Main, ALLOCATOR_ADD_OFFSET};

static TRAMPOLINE_BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/trampoline.bin"));

use x86_64::instructions::port::Port;
use core::ptr::{read_volatile, write_volatile};
use core::time::Duration;
use itertools::Itertools;
use uefi_raw::table::boot::MemoryType;
use x86_64::structures::paging::PageTable;
use crate::result::{Error, ErrorType};
use crate::timer::Timer;
use crate::timer::tsc::TSC;
use crate::uefi_helper::boot::MyMemoryMapOwned;

const LAPIC_ICR_LOW: *mut u32 = 0xfee00300 as *mut u32;
const LAPIC_ICR_HIGH: *mut u32 = 0xfee00310 as *mut u32;
const TRAMP_TARGET: usize = 0x7500;

unsafe fn wait_icr_idle() {
    while (read_volatile(LAPIC_ICR_LOW) & (1 << 12)) != 0 {
        core::hint::spin_loop();
    }
}

pub unsafe fn send_sipi(apic_id: u8, vector: u8) {
    TSC.spin(Duration::from_millis(10));

    let sipi_command = 0x00004600 | (vector as u32);
    for _ in 0..2 {unsafe {
        wait_icr_idle();
        write_volatile(LAPIC_ICR_HIGH, (apic_id as u32) << 24);
        write_volatile(LAPIC_ICR_LOW, sipi_command);
    }

        TSC.spin(Duration::from_micros(200));
    }
}

pub unsafe fn send_inits() {
    unsafe {
        wait_icr_idle();
        write_volatile(LAPIC_ICR_LOW, 0x000C4500);
    }
}

pub unsafe fn init_trampoline<const OVER_WRITE: bool>(entry_point: u64, pml4: &mut [PageTable], stack: &mut [*mut u8], uefi_map: MyMemoryMapOwned) -> result::Result {
    #[cfg(feature = "enable_normal_safety_checks")]
    if pml4.len() != stack.len() {
        return Error::new(
            ErrorType::InvalidArgument,
            Some("stacks.len() != pml4s.len()")
        ).raise();
    }

    #[cfg(feature = "enable_normal_safety_checks")]
    for (p4, st) in pml4.iter().zip(stack.iter()) {
        if st.is_null() || !st.addr().is_multiple_of(16) {
            return Error::new(
                ErrorType::InvalidArgument,
                Some("stack pointer is invalid.")
            ).raise();
        }

        #[cfg(feature = "enable_overprotective_safety_checks")]
        {
            if p4.is_empty() {
                return Error::new(
                    ErrorType::InvalidArgument,
                    Some("pml4/5 is empty.")
                ).raise();
            }
            let a = p4.iter();
        }
        #[cfg(not(feature = "enable_overprotective_safety_checks"))]
        let _ = p4;
    }

    #[cfg(feature = "enable_required_safety_checks")]
    {
        let mut ok = false;
        if let Some(item) = stack.last() {
            if item.addr() == 1 {
                ok = true;
            }
        }

        if !ok {
            return Error::new(
                ErrorType::InvalidArgument,
                Some("stack last item is need 0x1.")
            ).raise();
        }
    }

    let len = const {
        let len = TRAMPOLINE_BINARY.len();

        if TRAMP_TARGET < ALLOCATOR_ADD_OFFSET {
            panic!("Os allocator is likely to be corrupted during multi-core initialization.")
        }

        if len < 8192 {
            panic!("The check may not be working.")
        }

        len
    };

    #[cfg(feature = "enable_normal_safety_checks")]
    for i in uefi_map.iter() {
        let tramp_start = TRAMP_TARGET as u64;
        let tramp_end = (TRAMP_TARGET + len) as u64;
        let mem_start = i.phys_start;
        let mem_end = i.phys_start + i.page_count * 4096;

        if mem_start < tramp_end && tramp_start < mem_end {
            if i.ty != MemoryType::CONVENTIONAL {
                return Error::new(
                    ErrorType::AllocationFailed,
                    Some("The location for the trampoline cord is already in use.")
                ).raise();
            }
        }
    }

    #[cfg(feature = "enable_essential_safety_checks")]
    {
        let start = crate::memory::paging::get_addr(VirtAddr::new(TRAMP_TARGET as u64));
        let end = crate::memory::paging::get_addr(VirtAddr::new((TRAMP_TARGET + len) as u64));

        if start.is_err() || end.is_err() {
            return Error::new(
                ErrorType::InvalidData,
                Some("Mapping is required.")
            ).raise();
        }

        let start = start?;
        let end = end?;

        if TRAMP_TARGET != start.as_u64() as usize || TRAMP_TARGET + len != end.as_u64() as usize {
            return Error::new(
                ErrorType::NotSupported,
                Some("The scope of influence must be a 1:1 mapping.")
            ).raise();
        }
    }

    let args_ptr = unsafe { &mut *(TRAMP_TARGET as *mut TrampolineArgs) };

    #[cfg(feature = "enable_normal_safety_checks")]
    {
        if  !OVER_WRITE &&
            ((args_ptr.pml.addr()   == 0x56 &&
            args_ptr.stack.addr() == 0x72 &&
            args_ptr.target       == 0x85 &&
            args_ptr.frarg.addr() == 0x95 &&
            args_ptr.flags        == 0b10) ||
            args_ptr.safety       == 0x54855fafb595ad) {
            return Error::new(
                ErrorType::InternalError,
                Some("It has already been created.")
            ).raise();
        }
    }

    let dest = TRAMP_TARGET as *mut u8;
    unsafe {
        core::ptr::copy_nonoverlapping(
            TRAMPOLINE_BINARY.as_ptr(),
            dest,
            TRAMPOLINE_BINARY.len(),
        );
    }

    #[cfg(feature = "enable_normal_safety_checks")]
    {
        if  args_ptr.pml.addr()   != 0x56 ||
            args_ptr.stack.addr() != 0x72 ||
            args_ptr.target       != 0x85 ||
            args_ptr.frarg.addr() != 0x95 ||
            args_ptr.flags        != 0b10 {
            return Error::new(
                ErrorType::InternalError,
                Some("The health check failed.")
            ).raise();
        }
    }

    let mut flags = 0;
    if cpu_info!(current::paging::Pml5) { flags |= 0b10; }
    if cpu_info!(environment::paging::NX) { flags |= 0b1; }
    args_ptr.flags = flags;
    args_ptr.pml = pml4.as_mut_ptr();
    args_ptr.stack = stack.as_mut_ptr();
    args_ptr.target = entry_point;

    Ok(())
}

#[repr(C, align(8))]
struct TrampolineArgs {
    pml:    *mut PageTable,
    stack:  *mut *mut u8,
    target: u64,
    frarg:  *const Main,
    flags:  u8,
    safety: u64
}
