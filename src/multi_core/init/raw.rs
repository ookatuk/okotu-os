use alloc::vec;
use x86_64::VirtAddr;
use crate::{cpu_info, result, Main, ALLOCATOR_ADD_OFFSET, MAIN_COPY};

static TRAMPOLINE_BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/trampoline.bin"));

use uefi_raw::table::boot::MemoryType;
use x86_64::registers::control::{Cr3, Cr4};
use x86_64::structures::paging::PageTable;
use crate::memory::paging::PageEntryFlags;
use crate::result::{Error, ErrorType};
use crate::uefi_helper::boot::MyMemoryMapOwned;
use crate::util_types::MemRangeData;

const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

const TRAMP_TARGET: usize = 0x7500;


pub unsafe fn init_trampoline<const OVER_WRITE: bool>(entry_point: u64, stack: &mut [*mut u8], uefi_map: &MyMemoryMapOwned, pt: &PageTable) -> result::Result {
    #[cfg(feature = "enable_normal_safety_checks")]
    for (l, st) in stack.iter().enumerate() {
        let is_aligned = st.addr().is_multiple_of(16);
        let is_last_entry = l == stack.len() - 1;
        let is_canary = st.addr() == 1;

        let is_ok = is_aligned || (is_last_entry && is_canary);

        if st.is_null() || !is_ok {
            return Error::new(ErrorType::InvalidArgument, Some("stack pointer is invalid.")).raise();
        }
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

        if TRAMP_TARGET > ALLOCATOR_ADD_OFFSET {
            panic!("Os allocator is likely to be corrupted during multi-core initialization.")
        }

        if len < 8192 {
            panic!("The check may not be working.")
        }

        len
    };

    let main = MAIN_COPY.get().unwrap();
    let start = TRAMP_TARGET & !0xFFF;
    let end = align_up(TRAMP_TARGET + len, 4096);

    main.util_update_add_paging::<true>(
        vec![
            MemRangeData::new_start_end(start, end).unwrap()
        ],
        vec![
            PageEntryFlags::PRESENT | PageEntryFlags::WRITABLE,
        ]
    ).unwrap();

    #[cfg(feature = "enable_normal_safety_checks")]
    for i in uefi_map.iter() {
        let tramp_start = TRAMP_TARGET as u64;
        let tramp_end = (TRAMP_TARGET + len) as u64;
        let mem_start = i.phys_start;
        let mem_end = i.phys_start + i.page_count * 4096;

        if mem_start < tramp_end && tramp_start < mem_end {
            if i.ty == MemoryType::RUNTIME_SERVICES_CODE || i.ty == MemoryType::RUNTIME_SERVICES_DATA {
                return Error::new(
                    ErrorType::AllocationFailed,
                    Some("The location for the trampoline code is already in use.")
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
            ((args_ptr.tmp_table.addr()   == 0x56 &&
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
        if !((args_ptr.tmp_table.addr()   == 0x56 &&
                args_ptr.stack.addr() == 0x72 &&
                args_ptr.target       == 0x85 &&
                args_ptr.frarg.addr() == 0x95 &&
                args_ptr.flags        == 0b10) &&
                args_ptr.safety       == 0x54855fafb595ad) {
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
    args_ptr.stack = stack.as_mut_ptr();
    args_ptr.target = entry_point;
    args_ptr.tmp_table = Cr3::read().0.start_address().as_u64() as *const PageTable;
    args_ptr.cr4 = Cr4::read().bits();

    Ok(())
}

#[repr(C, align(16))]
struct TrampolineArgs {
    safety: u64,
    tmp_table: *const PageTable,
    stack:  *mut *mut u8,
    target: u64,
    frarg:  *const Main,
    cr4: u64,
    flags:  u8,
}
