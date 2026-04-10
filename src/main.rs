#![allow(incomplete_features)]

#![feature(likely_unlikely)]
#![feature(portable_simd)]
#![feature(const_trait_impl)]
#![feature(abi_x86_interrupt)]

#![feature(generic_const_exprs)]

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(all(
    not(all(target_arch = "x86_64", target_os = "uefi")),
    not(test)
))]
compile_error!("Unsupported target. Please use 'x86_64-unknown-uefi'.");

extern crate alloc;

const VERSION_RAW: &str = "1.0.0";

const MICRO_VER: u32 = 0;

const OS_NAME: &str = "okots";

/// OSプロトコルバージョン.
const DEBUG_PROTOCOL_VERSION: &str = "2.0";
#[allow(unused)]
const PANICED_TO_RESTART_TIME: usize = 20;
const STACK_SIZE: usize = 1024 * 32;
const ALLOCATOR_FIRST_CREATE_SIZE_OPTION_MAX_ALLOCATE_SIZE: usize = 1024 * 1024 * 1024 * 2;
const ALLOCATOR_FIRST_CREATE_SIZE_OPTION_MIN_ALLOCATE_SIZE: usize = 4096 * 2;

const ALLOCATOR_ADD_OFFSET: usize = 0x10000;

#[allow(unused)]
const POSITION_VALUE: u8 = 0x2F;

unsafe extern "C" {
    pub static __ImageBase: u8;
}

#[cfg(not(test))]
use alloc::{format};
#[cfg(not(test))]
use alloc::string::ToString;
#[cfg(not(test))]
use core::panic::PanicInfo;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use alloc::{vec};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::arch::{asm, naked_asm};
use core::ffi::c_void;
use core::hint::{spin_loop};
use core::sync::atomic::AtomicUsize;
use core::time::Duration;
use spin::{Once, RwLock};
use uefi::boot::{set_image_handle, AllocateType};
use uefi::mem::memory_map::MemoryMap;
use uefi::table::set_system_table;
use uefi_raw::table::boot::{MemoryType, PAGE_SIZE};
use x86_64::instructions::{hlt, interrupts};
use x86_64::instructions::interrupts::{enable, int3};
use x86_64::structures::paging::PageTable;
use crate::apic_helper::{broadcast_init_ipi_exc_self, broadcast_ipi_exc_self, send_init_ipi, send_sipi, ICR_STARTUP};
use crate::asy_nc::{pending, yield_now};
use crate::util::debug::with_interr;
use crate::cpu::cpu_id;
use crate::cpu::utils::{get_vendor_name_raw, vendor_list};
use crate::memory::paging::{PageEntryFlags, PageLevel, TopPageTable};
use crate::thread_local::read_gs;
use crate::timer::rtc::RTC;
use crate::timer::Timer;
use crate::timer::tsc::{Tsc, TSC};
use crate::uefi_helper::boot::MyMemoryMapOwned;
use crate::util::proto;
use crate::util_types::MemRangeData;

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
pub mod thread_local;
pub mod cpu;
pub mod cpu_flags;
pub mod acpi;
pub mod timer;
pub mod send_and_get;
pub mod drivers;
pub mod interrupt;
pub mod multi_core;
pub mod apic_helper;
pub mod asy_nc;

#[cfg_attr(not(test), global_allocator)]
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

#[derive(Default, Debug)]
pub struct GlobalData {
    pm_data: Once<(Vec<MemRangeData<usize>>, Vec<PageEntryFlags>)>,
    timer: AtomicBool,
    value: AtomicU64
}

#[derive(Default, Debug)]
struct Main {
    stack_data: RwLock<StackData>,
    core_info: RwLock<Vec<u32>>,
    uefi_map: Once<MyMemoryMapOwned>,
    initialized_core_count: AtomicUsize,
    global_data: GlobalData
}
static MAIN_COPY: Once<&'static Main> = Once::new();
static ASYNC_COPY: Once<&'static AsyncMain> = Once::new();

#[derive(Debug)]
pub struct AsyncMain {
    pub non_async_main: &'static &'static Main,
}

impl AsyncMain {
    pub fn new() -> Option<Self> {
        Some(AsyncMain {
            non_async_main: MAIN_COPY.get()?
        })
    }

    pub async fn main(&'static self) -> ! {
        log_last!("kernel", "kernel", "leached last.");
        loop {
            pending().await
        }
    }
}

impl Main {
    pub unsafe extern "C" fn main(&'static self, stack_top: u64, stack_len: u64) -> ! {
        if MAIN_COPY.is_completed() {
            log_last!("kernel", "kernel", "duplicate os main.");
            loop {
                hlt();
            };
        }

        MAIN_COPY.call_once(|| {self});

        with_interr(|| {
            let mut lock = self.stack_data.write();
            lock.top = stack_top as *mut u8;
            lock.len = stack_len as usize;
        });

        log_info!("kernel", "info", "initialing timer...");
        self.init_timers().expect("failed to initialize timers");
        log_info!("kernel", "info", "initialized timer...");

        self.get_core_info().expect("failed to get core infos.");

        log_info!("kernel", "info", "changing allocator...");
        self.change_allocator().expect("failed to enable os allocator.");
        log_info!("kernel", "info", "changed allocator.");

        self.exit_uefi().expect("failed to exit uefi.");

        acpi::core::get_acpi(unsafe{uefi::table::system_table_raw().unwrap().as_ref()}).expect("failed to get acpi.");

        cpu::utils::init_gdt();
        interrupt::api::init();

        apic_helper::init_local_apic();

        self.init_ap();

        TSC.spin(Duration::from_micros(10));

        let exec = asy_nc::Executor::new().unwrap();

        let asy = Box::leak(Box::new(AsyncMain::new().unwrap()));

        exec.spawn(AsyncMain::main(asy));

        exec.run();
    }

    fn init_ap(&'static self) {
        let mut len = with_interr(|| self.core_info.read().len());

        { // 起動
            let table = read_gs().unwrap().page_table.ptr();

            let mut stacks = Vec::with_capacity(len);

            for _ in 0..len {
                let ptr = unsafe { alloc::alloc::alloc(Layout::from_size_align(STACK_SIZE, 4096).unwrap()) };

                if ptr.is_null() {
                    panic!("failed to allocate stack.");
                }

                stacks.push(unsafe { ptr.add(STACK_SIZE) });
            }

            self.util_update_add_paging::<true>(
                stacks.iter().map(|x| {
                    let low = unsafe { x.sub(STACK_SIZE) };
                    MemRangeData::new(
                        low.addr(),
                        STACK_SIZE
                    )
                }).collect(),
                vec![PageEntryFlags::PRESENT | PageEntryFlags::WRITABLE; stacks.len()],
            ).unwrap();

            stacks.push(1 as *mut u8);

            unsafe { asm!("wbinvd", options(nomem, nostack)) };

            unsafe {
                multi_core::init::raw::init_trampoline::<false>(
                    Self::ap_entry_point as u64,
                    stacks.as_mut_slice(),
                    self.uefi_map.get().unwrap(),
                    table
                ).unwrap();
            }

            self.global_data.pm_data.call_once(|| {
                read_gs().unwrap().page_table.memory_mapping.clone()
            });

            unsafe { broadcast_init_ipi_exc_self() };
            TSC.spin(Duration::from_millis(10));
            for _ in 0..2 {
                unsafe { broadcast_ipi_exc_self(ICR_STARTUP, 0x08) };
                TSC.spin(Duration::from_micros(200));
            }

            let start = TSC.get_time() + Duration::from_secs(1);
            with_interr(|| {
                while self.initialized_core_count.load(Ordering::SeqCst) != len {
                    if TSC.get_time() > start {
                        len = self.initialized_core_count.load(Ordering::SeqCst);
                        break;
                    }
                    spin_loop();
                }
            });
            self.initialized_core_count.store(0, Ordering::SeqCst);
        }

        {  // タイマー設定
            self.global_data.value.store(0, Ordering::SeqCst);

            with_interr(|| {
                self.global_data.timer.store(true, Ordering::SeqCst);
                TSC.spin(Duration::from_millis(100));
                self.global_data.timer.store(false, Ordering::SeqCst);
            });

            while self.initialized_core_count.load(Ordering::SeqCst) < len {
                spin_loop();
            }

            self.global_data.value.store(Tsc::get(), Ordering::SeqCst);

            while self.initialized_core_count.load(Ordering::SeqCst) < len * 2 {
                spin_loop();
            }

            self.initialized_core_count.store(0, Ordering::SeqCst);
        }
    }

    pub fn ap_entry_point() -> ! {
        let me = MAIN_COPY.get().unwrap();

        me.ap_init();
    }

    pub fn ap_init(&'static self) -> ! {
        {
            unsafe { thread_local::write_none() };
            interrupt::api::init();
            interrupts::enable();

            let (mut a, mut b) = self.global_data.pm_data.get().unwrap().clone();
            self.create_paging(
                &mut a,
                &mut b,
            ).unwrap();
        }
        let gs = read_gs().unwrap();

        self.initialized_core_count.fetch_add(1, Ordering::SeqCst);

        {
            while !self.global_data.timer.load(Ordering::SeqCst) {
                spin_loop();
            }
            let start_tsc = Tsc::get();

            while self.global_data.timer.load(Ordering::SeqCst) {
                spin_loop();
            }

            let end_tsc = Tsc::get();
            let total_tsc = end_tsc - start_tsc;

            gs.tsc_data.par_100ns = total_tsc / 1_000_000;
        }

        apic_helper::init_local_apic();
        cpu::utils::init_gdt();
        interrupt::api::init();

        enable();

        {
            self.initialized_core_count.fetch_add(1, Ordering::SeqCst);

            let val = loop {
                let v = self.global_data.value.load(Ordering::SeqCst);
                if v != 0 { break v; }
                spin_loop();
            };

            let current = Tsc::get();
            gs.tsc_data.adjust = (val as i64) - (current as i64);

            self.initialized_core_count.fetch_add(1, Ordering::SeqCst);
        }

        gs.tsc_init = true;

        let exec = asy_nc::Executor::new().unwrap();

        exec.run();
    }

    fn get_core_info(&'static self) -> result::Result {
        extern "efiapi" fn internal(a: *mut c_void) {
            let me = unsafe { &*(a as *const Main) };

            with_interr(|| {
                let mut lock = me.core_info.write();
                let id = {
                    let mut ret = 0;
                    let vendor = get_vendor_name_raw().unwrap();

                    if vendor == vendor_list::AMD {
                        let max_ext = unsafe { cpu_id::read(0x80000000, None).eax };
                        if max_ext >= 0x8000001E {
                            let res = unsafe { cpu_id::read(0x8000001E, None) };
                            ret = res.eax;
                        }
                    } else if vendor == vendor_list::INTEL {
                        let res = unsafe { cpu_id::read(0x0B, Some(0)) };
                        if res.edx != 0 {
                            ret = res.edx;
                        }
                    }

                    if ret == 0 {
                        let res = unsafe { cpu_id::read(1, None) };
                        ret = (res.ebx >> 24) & 0xFF;
                    }

                    ret
                };
                lock.push(id);
            })
        }

        let pro = proto::open::<uefi::proto::pi::mp::MpServices>(None)?;
        with_interr(|| {
            let mut lock = self.core_info.write();
            lock.reserve(pro.get_number_of_processors().unwrap().enabled);
        });
        pro.startup_all_aps(
            false,
            internal,
            self as *const Self as *const c_void as *mut c_void,
            None,
            None
        )?;

        Ok(())
    }

    fn init_timers(&'static self) -> result::Result {
        log_info!("kernel", "timer", "initialing tsc...");
        TSC.init_for_ap(uefi::boot::stall, Duration::from_millis(100));
        log_info!("kernel", "timer", "initialing tsc utc time...");
        let think_utc = RTC.sync_and_get_time();
        TSC.option_init_time_stamp(think_utc);

        Ok(())
    }

    fn update_paging(&'static self, map: &MyMemoryMapOwned) -> result::Result<(Vec<MemRangeData<usize>>, Vec<PageEntryFlags>)> {
        let last = map.iter()
            .map(|e| e.phys_start as usize + e.page_count as usize * PAGE_SIZE)
            .max()
            .unwrap_or(0);

        let (mut r, mut t) = {
            let r = vec![
                PageEntryFlags::PRESENT | PageEntryFlags::WRITABLE | PageEntryFlags::EXECUTE_DISABLE
            ];
            let t = vec![MemRangeData::new(
                0,
                last
            )];

            (r, t)
        };

        for i in map.iter() {
            if i.ty == MemoryType::RUNTIME_SERVICES_CODE || i.ty == MemoryType::LOADER_CODE {
                let start = i.phys_start as usize;
                let len = i.page_count as usize * 4096;

                t.push(MemRangeData::new(start, len));
                r.push(PageEntryFlags::PRESENT | PageEntryFlags::WRITABLE);
            } else if i.ty == MemoryType::MMIO || i.ty == MemoryType::MMIO_PORT_SPACE {
                let start = i.phys_start as usize;
                let len = i.page_count as usize * 4096;

                t.push(MemRangeData::new(start, len));
                r.push(PageEntryFlags::PCD | PageEntryFlags::PRESENT | PageEntryFlags::WRITABLE | PageEntryFlags::EXECUTE_DISABLE);
            }
        }

        with_interr(|| {
            let lock = self.stack_data.read();
            t.push(MemRangeData::new(lock.top as usize, lock.len));
            r.push(PageEntryFlags::PRESENT | PageEntryFlags::WRITABLE | PageEntryFlags::EXECUTE_DISABLE);
        });

        t.push(MemRangeData::new(0, 4096));
        r.push(PageEntryFlags::empty());

        Ok((t, r))
    }

    fn exit_uefi(&'static self) -> result::Result {
        let map = unsafe{uefi_helper::boot::exit_boot_services_with_talc()};
        let map = self.uefi_map.call_once(|| {map});

        log_info!("kernel", "memory", "creating memory map...");

        let (mut t, mut r) = self.update_paging(&map)?;

        self.create_paging(&mut t, &mut r)?;

        log_info!("kernel", "memory", "adding other free memory...");

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

        with_interr(|| {
            let data = {
                let lock = ALLOC.os_allocator.lock();
                let counter = lock.counters();
                let meta_len = counter.claimed_bytes - counter.allocated_bytes - counter.available_bytes;
                lock.counters().total_claimed_bytes as usize - meta_len
            };
            log_custom!("s", "ds", "am", "{}", data);
        });

        log_info!("kernel", "memory", "added other free memory.");


        Ok(())
    }

    fn change_allocator(&'static self) -> result::Result {
        let mut current_attempt_size = ALLOCATOR_FIRST_CREATE_SIZE_OPTION_MAX_ALLOCATE_SIZE.min({
            let map = uefi::boot::memory_map(MemoryType::LOADER_DATA)?;
            let mut value = 0;

            for i in map.entries() {
                if i.ty != MemoryType::CONVENTIONAL {
                    continue;
                }
                value += i.page_count * 4096
            }

            value
        } as usize);

        loop {
            let request_size = current_attempt_size & !4095;
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

        with_interr(|| {
            let data = ALLOC.os_allocator.lock().counters().total_claimed_bytes;

            log_custom!("s", "ds", "am", "{}", data);
        });
        Ok(())
    }

    fn util_update_paging<const FREE: bool>(
        &'static self,
        map_list: &mut Vec<MemRangeData<usize>>,
        flags: &mut Vec<PageEntryFlags>,
    ) -> result::Result {
        let nx_mask = !PageEntryFlags::EXECUTE_DISABLE;

        let mut i = 0;
        flags.retain_mut(|f| {
            if !cpu_info!(environment::paging::NX) {
                *f &= nx_mask;
            }

            let keep = !f.is_empty();

            if !keep {
                map_list.remove(i);
            } else {
                i += 1;
            }

            keep
        });

        let mut old = read_gs().unwrap().page_table.clone();

        let res = memory::paging::update_paging(
            &mut old,
            map_list,
            flags,
            if cpu_info!(environment::paging::PdptHuge) {PageLevel::Pdpt} else {PageLevel::Pd},
        )?;



        if FREE {
            unsafe{memory::paging::free_not_used_paging(res)}
        };

        Ok(())
    }

    fn create_paging(
        &'static self,
        map_list: &mut Vec<MemRangeData<usize>>,
        flags: &mut Vec<PageEntryFlags>,
    ) -> result::Result {
        let nx_mask = !PageEntryFlags::EXECUTE_DISABLE;

        let mut i = 0;
        flags.retain_mut(|f| {
            if !cpu_info!(environment::paging::NX) {
                *f &= nx_mask;
            }

            let keep = !f.is_empty();

            if !keep {
                map_list.remove(i);
            } else {
                i += 1;
            }

            keep
        });

        let res = memory::paging::create_page_table(
            map_list,
            flags,
            if cpu_info!(environment::paging::PdptHuge) {PageLevel::Pdpt} else {PageLevel::Pd},
            if cpu_info!(current::paging::Pml5) {PageLevel::Pml5} else {PageLevel::Pml4},
        )?;

        memory::paging::set_current(&res);

        read_gs().unwrap().page_table = res;

        Ok(())
    }

    fn clone_paging(
        &'static self,
    ) -> result::Result<TopPageTable> {
        let nx_mask = !PageEntryFlags::EXECUTE_DISABLE;

        let old = &read_gs().unwrap().page_table.memory_mapping;

        let mut map_list = old.0.clone();

        let mut flags = old.1.clone();

        let mut i = 0;
        flags.retain_mut(|f| {
            if !cpu_info!(environment::paging::NX) {
                *f &= nx_mask;
            }

            let keep = !f.is_empty();

            if !keep {
                map_list.remove(i);
            } else {
                i += 1;
            }

            keep
        });

        let res = memory::paging::create_page_table(
            &mut map_list,
            &mut flags,
            if cpu_info!(environment::paging::PdptHuge) {PageLevel::Pdpt} else {PageLevel::Pd},
            if cpu_info!(current::paging::Pml5) {PageLevel::Pml5} else {PageLevel::Pml4},
        )?;

        Ok(res)
    }

    pub fn util_update_add_paging<const FREE: bool>(
        &'static self,
        mut map_list: Vec<MemRangeData<usize>>,
        mut flags: Vec<PageEntryFlags>
    ) -> result::Result {
        let old = &read_gs().unwrap().page_table.memory_mapping;

        let mut map = old.0.clone();
        map.append(&mut map_list);

        let mut flag = old.1.clone();
        flag.append(&mut flags);

        self.util_update_paging::<FREE>(&mut map, &mut flag)
    }
}

pub mod _internal_init {
    use crate::{io, log_custom, log_info, thread_local, Main, DEBUG_PROTOCOL_VERSION, STACK_SIZE};
    use core::alloc::Layout;
    use core::ptr;
    use uefi::runtime;

    use core::arch::asm;
    use crate::util::proto;
    use crate::version::OS_VERSION;

    pub unsafe extern "C" fn init_dep() {
        let _ = unsafe{thread_local::write_none()};

        #[cfg(feature = "enable_uart_outputs")]
        io::console::serial::init_serial();
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
        #[cfg(feature = "enable_lldb_debug")]
        unsafe {
            core::arch::asm!("int3");
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

    pub unsafe extern "C" fn allocate(target: *mut u64) {
        const _: () = {
            if STACK_SIZE % 4096 != 0 {
                panic!("you need stack size is aligned 4096.");
            }
        };

        let entropy = (get_boot_entropy() % 65536) & !0xf;
        let main_size = size_of::<Main>();
        let main_align = align_of::<Main>();


        let total_size = STACK_SIZE + entropy + main_size + main_align;
        let layout = Layout::from_size_align(total_size, 4096).unwrap();
        let allocated = unsafe { alloc::alloc::alloc_zeroed(layout) };

        if allocated.is_null() {
            unsafe { asm!("ud2", options(noreturn)) }
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
        "sub rsp, 56",              // rsp: *mut u8    -= (32 + 24)  // reserve 56-byte stack frame above current rsp

        "mov r12, rdx",             // r12: *mut u64  = rdx
        "call {set_handle}",        // set_handle(rcx: `func.args._handle`) // rcx to Clobbered

        "mov rcx, r12",             // rcx: *mut u64  = r12
        "call {set_table}",         // set_table(rcx: `func.args._table`)  // rcx to Clobbered

        "lea rcx, [rsp + 32]",      // rcx: *mut u64  = rsp.add(32) as u64

        "call {allocate}",          // allocate(mut rcx: `rsp.add(32)`)  // rcx to Clobbered
                                    // // rcx.add(0)       = u64  // stack_top
                                    // // rcx.add(1)       = u64  // *mut Main
                                    // // rcx.add(2)       = u64  // stack_len

        "mov r15, rsp",             // r15: *mut u64    = rsp

        "mov rdx, [rsp + 32]",      // rdx: &u64        = rsp + 32  // stack_top

        "and rdx, -16",             // rdx: *mut u64 &= -16 // rdx &= !15  // 16-bytes align

        "lea rsp, [rdx - 32]",      // rsp: *mut u8     = (rdx - 32) as *mut _ // move to new stack and reserve 32-byte stack

        "call {init_dep}",          // init_dep()

        "call {debug_hand_shake}",  // call_hand_shake()

        "mov rdx, [r15 + 32]",      // rdx: &u64        = r15 + 32  // stack_top
        "mov rcx, [r15 + 40]",      // rcx: &u64        = r15 + 40  // &'static self
        "mov r8, [r15 + 48]",       // r8 : &u64        = r15 + 48  // stack_len

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
