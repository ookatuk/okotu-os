#![feature(likely_unlikely)]
#![feature(portable_simd)]
#![feature(const_try)]

#![no_std]
#![no_main]

#[cfg(not(all(target_arch = "x86_64", target_os = "uefi")))]
compile_error!("Unsupported target. Please use 'x86_64-unknown-uefi'.");

extern crate alloc;

const VERSION_RAW: &str = "1.0.0";

const MICRO_VER: u32 = 0;

const OS_NAME: &str = "test_os_v2";

/// OSプロトコルバージョン.
const DEBUG_PROTOCOL_VERSION: &str = "2.0";

const PANICED_TO_RESTART_TIME: usize = 20;

const STACK_SIZE: usize = 1024 * 32;
const ALLOCATOR_FIRST_CREATE_SIZE_MAX: usize = 1024 * 1024 * 1024 * 2;
const ALLOCATOR_FIRST_CREATE_SIZE_OPTION_MAX_ALLOCATE_SIZE: usize = ALLOCATOR_FIRST_CREATE_SIZE_MAX;
const ALLOCATOR_FIRST_CREATE_SIZE_OPTION_MIN_ALLOCATE_SIZE: usize = 4096 * 2;

#[allow(unused)]
const POSITION_VALUE: u8 = 0x2F;


unsafe extern "C" {
    pub static __ImageBase: u8;
}

use alloc::{format, vec};
use alloc::string::ToString;
use alloc::vec::Vec;
use core::arch::{naked_asm};
use core::ffi::c_void;
use core::hint::{cold_path, spin_loop, unlikely};
use core::panic::PanicInfo;
use core::sync::atomic::Ordering;
use spin::{RwLock};
use uefi::boot::{set_image_handle, AllocateType};
use uefi::mem::memory_map::MemoryMap;
use uefi::table::set_system_table;
use uefi_raw::table::boot::{MemoryType, PAGE_SIZE};
use x86_64::instructions::interrupts::without_interrupts;
use crate::memory::paging::{PageEntryFlags, PageLevel};
use crate::result::{Error, ErrorType};
use crate::util_types::MemRangeData;
use crate::version::OS_VERSION;

mod io;
mod manager;
pub mod logger;
pub mod version;
pub mod util;
pub mod result;
pub mod simd;
pub mod memory;
pub mod util_types;
pub mod uefi_helper;

#[global_allocator]
/// 物理/仮想アロケーター.
pub static ALLOC: memory::physical_allocator::OsPhysicalAllocator = memory::physical_allocator::OsPhysicalAllocator::new();

#[derive(Debug, Default)]
struct StackData {
    pub top: *mut u8,
    pub len: usize,
}

#[derive(Default, Debug)]
pub struct ImagePtr {
    pub base: *mut u8,
    pub text: *mut u8,
    pub text_size: usize,
}

#[derive(Default)]
#[repr(align(16))]
struct Main {
    stack_data: RwLock<StackData>,
}

impl Main {
    pub unsafe extern "C" fn main(&'static self, _stack_top: u64, _stack_len: u64) -> ! {
        self.change_allocator().expect("failed to enable os allocator.");

        self.exit_uefi().expect("failed to exit uefi.");

        deb!("a");

        loop {
            spin_loop();
        }
    }

    fn exit_uefi(&'static self) -> result::Result {
        let map = unsafe{uefi_helper::boot::exit_boot_services_with_talc()};
        let last = map.iter()
            .map(|e| e.phys_start as usize + e.page_count as usize * PAGE_SIZE)
            .max()
            .unwrap_or(0);

        let res = memory::paging::create_page_table(
            &mut vec![MemRangeData::new(
                0,
                last
            )],
            &mut vec![
                PageEntryFlags::PRESENT | PageEntryFlags::WRITABLE
            ],
            PageLevel::Pdpt,
            PageLevel::Pml4
        )?;
        memory::paging::set_current(res);

        for i in map.iter() {
            if i.ty == MemoryType::BOOT_SERVICES_CODE || i.ty == MemoryType::BOOT_SERVICES_DATA {
                let start = i.phys_start as usize;
                let len = i.page_count as usize * 4096;

                unsafe {
                    ALLOC.add_target_to_os_alloc(
                        MemRangeData::new(start, len),
                    );
                }
            }
        }

        without_interrupts(|| {
            let data = {
                let lock = ALLOC.os_allocator.lock();
                let counter = lock.counters();
                let meta_len = counter.claimed_bytes - counter.allocated_bytes - counter.available_bytes;
                lock.counters().total_claimed_bytes as usize - meta_len
            };
            log_custom!("s", "ds", "am", "{}", data);
        });

        Ok(())
    }

    fn change_allocator(&'static self) -> result::Result {
        let mut current_attempt_size = usize::MAX;

        loop {
            let request_size = current_attempt_size;
            let pages = request_size / 4096;

            if let Ok(ptr) = uefi::boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages) {
                let range = MemRangeData::new(ptr.addr().get(), request_size);
                unsafe { ALLOC.add_target_to_os_alloc(range) };
            } else {
                current_attempt_size /= 2;

                if current_attempt_size < 4096 {
                    current_attempt_size = 4096;
                }

                if current_attempt_size < ALLOCATOR_FIRST_CREATE_SIZE_OPTION_MIN_ALLOCATE_SIZE {
                    break;
                }
            }
        }

        unsafe{ALLOC.change_to_os_allocator()};

        Ok(())
    }
}

pub mod _internal_init {
    use crate::{io, log_custom, log_info, Main, DEBUG_PROTOCOL_VERSION, STACK_SIZE};
    use core::alloc::Layout;
    use core::ptr;
    use uefi::runtime;

    #[cfg(feature = "enable_lldb_debug")]
    use core::arch::asm;
    use crate::util::proto;
    use crate::version::OS_VERSION;

    #[inline(always)]
    pub unsafe extern "C" fn init_dep() {
        #[cfg(feature = "enable_uart_outputs")]
        io::console::serial::init_serial();
        uefi::helpers::init().expect("Failed to init uefi helpers");
    }

    #[inline(always)]
    pub fn get_boot_entropy() -> usize {
        let mut entropy: usize = 0;

        if let Ok(mut rng_proto) = proto::open::<uefi::proto::rng::Rng>(None) {
            let mut buf = [0u8; size_of::<usize>()];
            if rng_proto.get_rng(None, &mut buf).is_ok() {
                entropy = usize::from_le_bytes(buf);
            }
        }

        let tsc = unsafe { core::arch::x86_64::_rdtsc() as usize };

        let time_val = runtime::get_time()
            .map(|t| t.nanosecond() as usize)
            .unwrap_or(0);

        entropy ^ tsc ^ time_val
    }

    #[inline(always)]
    pub unsafe extern "C" fn debug_hand() {
        if cfg!(feature = "enable_lldb_debug") {
            unsafe {
                core::arch::asm!("int3");
            }
        }

        log_custom!("s", "ds", "a", "");
        log_custom!(
            "s",
            "ds",
            "d",
            "{}",
            if cfg!(any(
                feature = "enable_debug_outputs",
                feature = "enable_debug_level_outputs"
            )) {
                1
            } else {
                0
            }
        );

        log_custom!("s", "ds", "v", "{}", *OS_VERSION);
        log_custom!("s", "ds", "pv", "{}", DEBUG_PROTOCOL_VERSION);

        log_info!("debug", "build info", "{}", yaml_peg::serde::to_string(&*OS_VERSION).unwrap());
    }

    #[inline(always)]
    pub unsafe extern "C" fn allocate(target: *mut u64) {
        let entropy = (get_boot_entropy() % 65536) & !0xf;
        let main_size = size_of::<Main>();
        let main_align = align_of::<Main>();

        let total_size = STACK_SIZE + entropy + main_size + main_align;
        let layout = Layout::from_size_align(total_size, 4096).unwrap();
        let allocated = unsafe { alloc::alloc::alloc_zeroed(layout) };

        if allocated.is_null() {
            panic!("Allocation failed");
        }

        let stack_top = unsafe { allocated.add(STACK_SIZE) as usize } & !0xf;

        let struct_addr = (stack_top + entropy + main_align) & !(main_align - 1);
        let struct_ptr = struct_addr as *mut Main;

        unsafe {
            ptr::write(struct_ptr, Main::default());
        }

        unsafe {
            *target.offset(0) = stack_top as u64;
            *target.offset(1) = struct_ptr as u64;
            *target.offset(2) = STACK_SIZE as u64;
        }
    }
}

#[unsafe(naked)]
#[unsafe(export_name = "efi_main")]
pub extern "efiapi" fn efi_main(_handle: uefi::Handle, _table: *mut c_void) -> ! {
    // Generally, the way arguments and return values are handled is called the "ABI",
    // and the most central one is the "C ABI".

    // In C abi,
    // the first argument must be in the "rcx" register,
    // the second argument in the "rdx" register,
    // and the third argument in the "r8" register.

    // The rsp in C abi,
    // called stack manager register,
    // requires 32 bytes of space,
    // sometimes called a "shadow stack",
    // which is a multiple of 16 when the function is executed(if use `call`).

    // // *const <T>: not mutable pointer (read only)
    // // *mut   <T>:     mutable pointer (read and write)

    naked_asm!(
        "endbr64",                  // cet::allow_jump() //for CET (Control-flow Enforcement Technology) instructions

                                    // let eax: *mut u64;
                                    // let r12: *mut u64;
                                    // let rdx: *mut u64;

                                    // // It's not actually true, but it's roughly like this
                                    // ```
                                    //     let mut rcx: *mut u64 = func.args._handle
                                    //     let mut rdx: *mutcargo u64 = func.args._table

                                    //     let mut rsp: *mut u8    = get_stack_pointer!().addr()
                                    //     let mut gs : *mut u16 = get_gs_register!().addr()
                                    // ```
        "xor eax, eax",             // eax: *mut u64  = 0
        "mov gs, ax",               // gs : *mut u16  = eax as u16

        "sub rsp, 56",              // rsp: *mut u8    -= 56  // reserve 56-byte stack frame above current rsp

        "mov r12, rdx",             // r12: *mut u64  = rdx
        "call {set_handle}",        // set_handle(rcx: `func.args._handle`) // rcx to Clobbered

        "mov rcx, r12",             // rcx: *mut u64  = r12
        "call {set_table}",         // set_table(rcx: `func.args._table`)  // rcx to Clobbered

        "call {init_dep}",          // init_dep()

        "call {debug_hand_shake}",  // call_hand_shake()

        "lea rcx, [rsp + 32]",      // rcx: *mut u64  = rsp.add(32) as u64

        "call {allocate}",          // allocate(mut rcx: `rsp.add(32)`)  // rcx to Clobbered
                                    // // rcx.add(0)       = u64  // stack_top
                                    // // rcx.add(1)       = u64  // *mut Main
                                    // // rcx.add(2)       = u64  // stack_len

        "mov rdx, [rsp + 32]",      // rdx: &u64        = rsp + 32  // stack_top
        "mov rcx, [rsp + 40]",      // rcx: &u64        = rsp + 40  // &'static self
        "mov r8, [rsp + 48]",       // r8 : &u64        = rsp + 48  // stack_len

        "and rdx, -16",             // rdx: *mut u64 &= -16 // rdx &= !15  // 16-bytes align

        "lea rsp, [rdx - 32]",      // rsp: *mut u8     = (rdx - 32) as *mut _ // move to new stack and reserve 32-byte stack

        "jmp {main}",               // main(rcx as &Main, rdx, r8)  // main(rcx: &Main, rdx: u64, r8: u64) -> !
        init_dep = sym _internal_init::init_dep,
        debug_hand_shake = sym _internal_init::debug_hand,
        allocate = sym _internal_init::allocate,
        main = sym Main::main,
        set_handle = sym set_image_handle,
        set_table = sym set_system_table,
    )
}

#[panic_handler]
#[cfg(not(test))]
fn panic(info: &PanicInfo) -> ! {
    let message = info.to_string();
    let loc = info.location().unwrap().to_string();

    logger::core::LOG_CAPACITY.store(0, Ordering::SeqCst);

    log_last!("kernel", "panic", "panic raised.");
    log_last!("kernel", "panic", "version: {:?}", *OS_VERSION);

    log_last!("kernel", "panic", "{}\n{}", loc, message);

    #[cfg(not(feature = "disable_panic_restarts"))]
    let tmp_text = format!(
        "System will restart in {} seconds. ",
        PANICED_TO_RESTART_TIME
    );
    #[cfg(feature = "disable_panic_restarts")]
    let tmp_text = "".to_string();

    log_last!(
        "kernel",
        "panic",
        "A critical system error has occurred. {}for system admin: (info: {}, by: {})",
        tmp_text,
        info.message(),
        info.location().unwrap()
    );

    loop {
        spin_loop()
    }
}

unsafe impl Send for Main {}
unsafe impl Sync for Main {}
