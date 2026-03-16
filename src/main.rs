#![feature(abi_x86_interrupt)]
#![feature(const_slice_make_iter)]

#![no_std]
#![no_main]

extern crate alloc;

const VERSION_RAW: &str = "1.0.0";

const MICRO_VER: u32 = 0;

const OS_NAME: &str = "test_os_v2";

/// OSプロトコルバージョン.
const DEBUG_PROTOCOL_VERSION: &str = "2.0";

const LINE_SPACING: f32 = 1.5;

const MAX_DO_ITEM: usize = 1000;

const BAR_HEIGHT: usize = 20;
const BAR_MARGIN: usize = 50;

const GUI_WAIT: usize = 2_000_000;

const PANICED_TO_RESTART_TIME: usize = 20;

const ALLOW_RATIOS: &[(usize, usize)] =
    &[(21, 9), (32, 9), (16, 9), (16, 10), (4, 3), (3, 2), (5, 4)];

static MAIN_FONT: Once<Box<[u8]>> = Once::new();

const POSITION_VALUE: u8 = 0x2F;

unsafe extern "C" {
    static __ImageBase: u8;
}

use crate::io::console::gop::Color;
use crate::manager::display_manager::DisplayManager;
use crate::manager::load_task_manager::LoadTaskManager;
use crate::manager::memory_manager::MemoryManager;
use crate::mem::allocator::main::OsAllocator;
use crate::mem::map::MemoryMapType;
use crate::mem::paging::types::{PageEntryFlags, PageLevel, get_addr};
use crate::mem::types::{MemData, MemMap};
use crate::util::result::Error;
use crate::util::timer::TSC;
use acpi::sdt::hpet::HpetTable;
use acpi::sdt::madt::{Madt, MadtEntry};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::{format, vec};
use bitflags::bitflags;
use const_format::formatcp;
use core::alloc::Layout;
use core::arch::x86_64::_rdrand64_step;
use core::arch::{asm, naked_asm};
use core::cmp::PartialEq;
use core::ffi::c_void;
use core::hint::spin_loop;
use core::num::NonZeroUsize;
use core::panic::PanicInfo;
use core::pin::pin;
use core::ptr::{NonNull, addr_of, null_mut};
use core::sync::atomic::Ordering;
use core::time::Duration;
use fontdue::Font;
use num_traits::Zero;
use spin::{Once, RwLock};
use uefi::boot::{AllocateType, TimerTrigger, set_image_handle};
use uefi::proto::console::text::Key;
use uefi::table::set_system_table;
use uefi::{CStr16, boot, cstr16, entry};
use uefi_raw::table::boot::{EventType, Tpl};
use uefi_raw::table::runtime::ResetType;
use uefi_raw::{PhysicalAddress, Status};
use util::result;
use x86_64::VirtAddr;
use x86_64::instructions::interrupts::without_interrupts;
use x86_64::instructions::{interrupts, tlb};
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::registers::model_specific::Msr;
use x86_64::structures::paging::{PageTable, PhysFrame};

mod cpu;
mod fonts;
mod fs;
mod io;
mod manager;
mod mem;
mod rng;
mod table;
mod util;

#[global_allocator]
/// 物理/仮想アロケーター.
pub static ALLOC: OsAllocator = OsAllocator::new();

static MAIN_PTR: Once<&'static Main> = Once::new();

bitflags! {
    #[derive(Debug)]
    pub struct State: u8 {
        const DEST = 1 << 0;
        const RUNNING = 1 << 1;
        const ERR = 1 << 2;
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum BootMode {
    Normal,
    Reboot,
    #[cfg(feature = "include_boot_option_to_memcheck")]
    MemCheck,
}

#[derive(Debug)]
struct WatchingItem {
    pub ptr: u64,
    pub reversed_ptr: u64,
    pub level: u8,
    pub state: State,
}

impl WatchingItem {
    #[inline]
    pub fn new<T>(ptr: fn(&T) -> result::Result, level: u8) -> Self {
        let addr = ptr as usize; // ここで数値化
        Self {
            ptr: (addr ^ level as usize ^ 7) as u64,
            reversed_ptr: (addr.reverse_bits()) as u64,
            level,
            state: State::empty(),
        }
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        self.ptr.is_zero()
    }

    #[inline]
    pub fn is_dest(&self) -> bool {
        self.state.contains(State::DEST)
    }

    #[inline]
    pub fn is_running(&self) -> bool {
        self.state.contains(State::RUNNING)
    }

    #[inline]
    pub fn is_err(&self) -> bool {
        self.state.contains(State::ERR)
    }
}

#[derive(Debug, Default)]
struct StackData {
    pub top: *mut u8,
    pub len: usize,
}

#[derive(Debug, Default)]
struct Helpers {}

#[derive(Default)]
#[repr(align(16))]
struct Main {
    global_font: Arc<RwLock<Option<Font>>>,

    stack_data: RwLock<StackData>,

    do_fn: Once<Arc<dyn Fn()>>,

    load_task_manager: Arc<LoadTaskManager>,

    display_manager: DisplayManager,
    memory_manager: MemoryManager,

    helpers: Helpers,
}

impl Main {
    fn init_font(&self) -> result::Result<()> {
        let font = fonts::load()?;

        MAIN_FONT.call_once(|| font);

        let new_font = Box::new(fonts::load_font(unsafe { MAIN_FONT.get_unchecked() }));
        interrupts::without_interrupts(|| {
            let mut a = self.global_font.write();
            *a = Some(*new_font);
        });

        Ok(())
    }

    fn frist_init(&self) -> Vec<result::Result> {
        let mut ret = vec![];

        self.display_manager
            .global_font
            .call_once(|| self.global_font.clone());
        self.load_task_manager
            .do_parent
            .call_once(|| self.display_manager.do_parent.clone());

        let ltm = Arc::clone(&self.load_task_manager);

        self.do_fn.call_once(|| ltm.get_add_func());

        self.memory_manager
            .do_fn
            .call_once(|| self.do_fn.get().unwrap().clone());

        without_interrupts(|| {
            ret.push(TSC.write().init(None));
        });

        ret.push(cpu::mitigation::ucode::load());

        ret.push(self.init_font());

        ret.push(self.display_manager.init_gop());

        ret
    }

    #[cfg(feature = "enable_stack_checks")]
    pub extern "efiapi" fn check_canaria(_event: uefi::Event, context: Option<NonNull<c_void>>) {
        let me = unsafe { &*(context.unwrap().as_ptr() as *mut Main) };

        let (stack_top, stack_len) = without_interrupts(|| {
            let lock = me.stack_data.try_read();
            if lock.is_none() {
                return (None, None);
            }
            let lock = lock.unwrap();

            (Some(lock.top), Some(lock.len))
        });

        if stack_len.is_none() {
            return;
        }

        let (stack_top, stack_len) = (stack_top.unwrap(), stack_len.unwrap());

        let stack_top = stack_top.addr();

        let stack_bottom = stack_top - stack_len;

        if unsafe { *(stack_bottom as *mut u64) } != 0x5555_AAAA_5555_AAAA {
            unsafe {
                asm!(
                "out dx, al",
                in("dx") 0x3f8_u16,
                in("al") b'\xFF',
                options(nomem, nostack, preserves_flags)
                )
            };
            unsafe { asm!("ud2", options(noreturn)) };
        }
    }

    #[cfg(feature = "enable_stack_checks")]
    fn enable_stack_canaria(&self) {
        log_info!("kernel", "canaria", "creating stack canaria");
        let (stack_top, stack_len) = without_interrupts(|| {
            let lock = self.stack_data.read();
            (lock.top, lock.len)
        });

        let stack_top = stack_top.addr();

        let stack_bottom = stack_top - stack_len;

        unsafe {
            *(stack_bottom as *mut u64) = 0x5555_AAAA_5555_AAAA;
        }

        let self_ptr = NonNull::new(core::ptr::addr_of!(*self) as *mut c_void);

        log_info!("kernel", "canaria", "setting event...");

        let event = unsafe {
            boot::create_event(
                EventType::TIMER | EventType::NOTIFY_SIGNAL,
                Tpl::NOTIFY,
                Some(Self::check_canaria),
                self_ptr,
            )
            .unwrap()
        };

        Error::try_raise(
            uefi::boot::set_timer(&event, TimerTrigger::Periodic(100_000)),
            Some("failed to set timer periodic event."),
        )
        .unwrap();

        log_info!("kernel", "canaria", "created.");
    }

    fn get_boot_mode(&self) -> result::Result<BootMode> {
        let mut internal_gop = self.display_manager.gop_data.write();
        let mut gop = internal_gop.as_mut().unwrap();

        let mut st_o = util::proto::open::<uefi::proto::console::text::Output>(None)?;
        let mut st_i = util::proto::open::<uefi::proto::console::text::Input>(None)?;

        unsafe { gop.clear(Color::new(0.0, 0.0, 0.0)) }?;

        let mut pr = |data: &CStr16| -> result::Result {
            let a = st_o.get_mut().unwrap();
            a.output_string(data)?;
            Ok(())
        };

        pr(cstr16!("--- Boot Menu ---\r\n"))?;
        pr(cstr16!("1. Normal Boot\r\n"))?;
        pr(cstr16!("2. Reboot\r\n"))?;
        #[cfg(feature = "include_boot_option_to_memcheck")]
        pr(cstr16!("3. Memory Check (Built-in)\r\n"))?;
        pr(cstr16!("-----------------\r\n"))?;
        pr(cstr16!("Select: "))?;
        let mode = loop {
            boot::wait_for_event(&mut [st_i.wait_for_key_event().unwrap()]).unwrap();

            let key = st_i.read_key()?.expect("Key should be present");
            if let Key::Printable(k) = key {
                let c: char = k.into();

                match c {
                    '1' => break BootMode::Normal,
                    #[cfg(feature = "include_boot_option_to_memcheck")]
                    '2' => break BootMode::MemCheck,
                    '3' => break BootMode::Reboot,
                    _ => continue,
                }
            }
        };

        Ok(mode)
    }

    #[cfg(feature = "include_boot_option_to_memcheck")]
    fn memcheck(&self) -> ! {
        log_info!(
            "kernel",
            "memcheck",
            "memory check started. It will take some time."
        );

        let default = util::logger::LOG_CAPACITY.load(Ordering::SeqCst);
        util::logger::LOG_CAPACITY.store(0, Ordering::SeqCst);
        let _ = uefi::boot::set_watchdog_timer(0, 0, None);

        unsafe fn flush_range(start: *mut u64, size_bytes: usize) {
            let mut ptr = start as usize;
            let end = ptr + size_bytes;
            while ptr < end {
                asm!("clflush [{0}]", in(reg) ptr);
                ptr += 64;
            }
            asm!("mfence");
        }

        let mut error_log = vec![];
        let map = without_interrupts(|| {
            *self.load_task_manager.do_parent.get().unwrap().write() = 0.0;
            self.memory_manager
                .uefi_memory_map
                .get()
                .unwrap()
                .read()
                .clone()
        });

        let patterns: [Option<u8>; 5] = [Some(0), Some(0xff), Some(0x55), Some(0xaa), None];

        // 全体サイズ計算 (4KiB単位で精密に)
        let mut total_size: f64 = 0.0;
        for i in map.0.iter() {
            if i.memory_type == MemoryMapType::NotAllocatedByUefiAllocator {
                let s = (i.data.start + 0xFFF) & !0xFFF;
                let e = i.data.end & !0xFFF;
                if s < e {
                    total_size += (e - s) as f64;
                }
            }
        }
        let progress_denominator = total_size * patterns.len() as f64;

        for &p_opt in &patterns {
            for descriptor in map.0.iter() {
                if descriptor.memory_type != MemoryMapType::NotAllocatedByUefiAllocator {
                    continue;
                }

                let range_start = (descriptor.data.start + 0xFFF) & !0xFFF;
                let range_end = descriptor.data.end & !0xFFF;

                let mut current_pos = range_start;
                let mut old: u64 = 0;

                while current_pos < range_end {
                    let remaining = range_end - current_pos;

                    let (alloc_pages, is_large) = if remaining >= 2 * 1024 * 1024 {
                        (512, true)
                    } else {
                        (1, false)
                    };

                    if old < current_pos {
                        let parent = without_interrupts(|| {
                            *self.load_task_manager.do_parent.get().unwrap().read()
                        });

                        let val = match p_opt {
                            Some(u) => u.to_string(),
                            None => "ptr".to_string(),
                        };

                        log_info!(
                            "kernel",
                            "memcheck",
                            "({} %) ({}) checking {:#X}",
                            (parent * 100.0) as u8,
                            val,
                            current_pos
                        );
                        old = current_pos + (1024 * 1024 * 100);
                    }

                    let mut res = uefi::boot::allocate_pages(
                        AllocateType::Address(current_pos as PhysicalAddress),
                        uefi::boot::MemoryType::LOADER_DATA,
                        alloc_pages,
                    );

                    if res.is_err() && is_large {
                        res = uefi::boot::allocate_pages(
                            AllocateType::Address(current_pos as PhysicalAddress),
                            uefi::boot::MemoryType::LOADER_DATA,
                            1,
                        );
                    }

                    let actual_pages = if res.is_ok() {
                        if is_large && res.is_ok() { 512 } else { 1 }
                    } else {
                        current_pos += if is_large { 2 * 1024 * 1024 } else { 4 * 1024 };
                        self.update_memcheck_progress(
                            if is_large { 2 * 1024 * 1024 } else { 4 * 1024 },
                            progress_denominator,
                        );
                        continue;
                    };

                    let check_len = actual_pages * 4096;
                    let mb_start = current_pos as *mut u64;
                    let mb_end = (current_pos + check_len) as *mut u64;

                    let mut ptr = mb_start;
                    while ptr < mb_end {
                        let val = match p_opt {
                            Some(u) => u64::from_ne_bytes([u; 8]),
                            None => ptr.addr() as u64,
                        };
                        unsafe { ptr.write_volatile(val) };
                        ptr = unsafe { ptr.add(1) };
                    }

                    unsafe { flush_range(mb_start, check_len as usize) };
                    ptr = mb_start;
                    while ptr < mb_end {
                        let expected = match p_opt {
                            Some(u) => u64::from_ne_bytes([u; 8]),
                            None => ptr.addr() as u64,
                        };
                        let actual = unsafe { ptr.read_volatile() };
                        if actual != expected {
                            if !error_log.contains(&ptr.addr()) {
                                error_log.push(ptr.addr());
                            }
                        }
                        unsafe { ptr.write_volatile(0) };
                        ptr = unsafe { ptr.add(1) };
                    }

                    self.update_memcheck_progress(check_len as usize, progress_denominator);

                    let ptr = core::ptr::NonNull::new(current_pos as *mut u8).unwrap();
                    let _ = unsafe { uefi::boot::free_pages(ptr, actual_pages as usize) };
                    current_pos += check_len;
                }
            }
        }

        util::logger::LOG_CAPACITY.store(default, Ordering::SeqCst);

        // メッセージ表示の変更
        if !error_log.is_empty() {
            log_warn!(
                "kernel",
                "memcheck",
                "press key to exit. broken detect: {:?}",
                error_log
            );
        } else {
            log_info!(
                "kernel",
                "memcheck",
                "press key to exit. memory check success."
            );
        }

        let st_i = util::proto::open::<uefi::proto::console::text::Input>(None).unwrap();
        boot::wait_for_event(&mut [st_i.wait_for_key_event().unwrap()]).unwrap();
        uefi::runtime::reset(ResetType::COLD, Status::SUCCESS, None);
    }

    fn update_memcheck_progress(&self, bytes: usize, denominator: f64) {
        without_interrupts(|| {
            let mut a = self.load_task_manager.do_parent.get().unwrap().write();
            *a += bytes as f64 / denominator;
        });
    }

    fn exit_uefi(&self) -> result::Result {
        let (gop_ptr, gop_len) = without_interrupts(|| {
            let lock = self.display_manager.gop_data.read();
            let data = lock.as_ref().unwrap();

            let raw_ptr = data.ptr.unwrap().addr().get();
            let raw_len = data.h.get() * data.stride.get() * 4;

            let aligned_ptr = raw_ptr & !4095;

            let offset = raw_ptr - aligned_ptr;

            let aligned_len = (raw_len + offset + 4095) & !4095;

            (aligned_ptr, aligned_len)
        });

        self.do_fn.get().unwrap()();

        unsafe {
            let event = {
                self.display_manager
                    .gop_uefi_event
                    .get()
                    .unwrap()
                    .unsafe_clone()
            };

            let _ = boot::close_event(event);

            let _ = boot::exit_boot_services(None);
        }

        self.do_fn.get().unwrap()();

        self.display_manager.do_load_grap_in_now();

        log_debug!("kernel", "paging", "creating paging data...");

        let mut codes = without_interrupts(|| {
            let mut data = vec![];

            let a = self.memory_manager.uefi_memory_map.get().unwrap().read();
            for i in a.0.iter() {
                if i.memory_type == MemoryMapType::KernelCode
                    || i.memory_type == MemoryMapType::UefiRuntimeServiceCode
                {
                    let raw_start = i.data.start as usize;
                    let raw_end = i.data.end as usize;

                    let aligned_start = raw_start & !0xfff;
                    let aligned_end = (raw_end + 0xfff) & !0xfff;

                    let len = aligned_end - aligned_start;
                    if len < 4096 {
                        continue;
                    }

                    data.push(MemData::<usize> {
                        start: aligned_start,
                        len,
                    })
                }
            }

            data
        });

        let mut list = vec![
            MemData {
                start: 0,
                len: self.memory_manager.max_addr.load(Ordering::SeqCst),
            },
            MemData {
                start: gop_ptr,
                len: gop_len,
            },
        ];

        let mut flag = vec![
            PageEntryFlags::WRITABLE | PageEntryFlags::EXECUTE_DISABLE,
            PageEntryFlags::WRITABLE
                | PageEntryFlags::PRESENT
                | PageEntryFlags::EXECUTE_DISABLE
                | PageEntryFlags::PAT, // gop
        ];

        {
            let len = codes.len();
            list.append(&mut codes);
            flag.reserve(len);
            for _ in 0..len {
                flag.push(PageEntryFlags::PRESENT | PageEntryFlags::WRITABLE); // kernel code/loader_code
            }

            let vec = self.memory_manager.uefi_memory_map.get().unwrap().read();

            for i in vec.0.iter() {
                if i.memory_type != MemoryMapType::NotAllocatedByUefiAllocator && // アロケーターの範囲/アロケート済み範囲
                    i.memory_type != MemoryMapType::UefiBootServicesAllocated &&

                    i.memory_type != MemoryMapType::KernelData &&  // data
                    i.memory_type != MemoryMapType::UefiRuntimeServiceAllocated &&

                    i.memory_type != MemoryMapType::AcpiTable &&  // acpi関係
                    i.memory_type != MemoryMapType::Acpi
                {
                    continue;
                }

                let raw_start = i.data.start as usize;
                let raw_end = i.data.end as usize;

                let aligned_start = raw_start & !0xfff;
                let aligned_end = (raw_end + 0xfff) & !0xfff;

                let len = aligned_end - aligned_start;
                if len < 4096 {
                    continue;
                }

                flag.push(
                    PageEntryFlags::PRESENT
                        | PageEntryFlags::WRITABLE
                        | PageEntryFlags::EXECUTE_DISABLE,
                );
                list.push(MemData {
                    start: aligned_start,
                    len,
                });
            }

            for i in vec.0.iter() {
                if i.memory_type != MemoryMapType::Mmio {
                    continue;
                }

                let raw_start = i.data.start as usize;
                let raw_end = i.data.end as usize;

                let aligned_start = raw_start & !0xfff;
                let aligned_end = (raw_end + 0xfff) & !0xfff;

                let len = aligned_end - aligned_start;
                if len < 4096 {
                    continue;
                }

                flag.push(
                    PageEntryFlags::PRESENT
                        | PageEntryFlags::WRITABLE
                        | PageEntryFlags::EXECUTE_DISABLE
                        | PageEntryFlags::PAT
                        | PageEntryFlags::PCD,
                );
                list.push(MemData {
                    start: aligned_start,
                    len,
                });
            }
        }

        self.do_fn.get().unwrap()();

        unsafe {
            let mut pat_msr = Msr::new(0x277);
            let mut pat = pat_msr.read();

            pat &= !(0b111 << 32);
            pat |= (0x01 << 32);

            pat &= !(0b111 << 24);
            pat |= (0x00 << 24);

            pat_msr.write(pat);
        }
        let a = Cr3::read().0.start_address().as_u64() as *mut PageTable;

        log_debug!("kernel", "paging", "creating paging...");

        let page = mem::paging::types::create_page_table(
            &mut list,
            &mut flag,
            PageLevel::Pdpt,
            PageLevel::Pml4,
            unsafe { &mut *a },
        )?;

        self.do_fn.get().unwrap()();

        log_debug!("kernel", "paging", "Registering...");

        unsafe { asm!("WBINVD") }

        let page_frame = PhysFrame::containing_address(page.phys);

        mem::paging::types::set_current(page);
        unsafe {
            Cr3::write(page_frame, Cr3Flags::empty());
        }

        tlb::flush_all();

        log_debug!("kernel", "paging", "Registered");

        self.do_fn.get().unwrap()();

        // 再作成前だとwritableとかが問題だから再作成後に実行
        let res = self
            .memory_manager
            .add_allocators(&[MemoryMapType::UefiBootServicesAllocated]);

        if let Err(res) = res {
            log_warn!(
                "kernel",
                "memory",
                "failed to add boot services allocated memory. ({})",
                res
            )
        }

        self.do_fn.get().unwrap()();

        log_custom!(
            "s",
            "ds",
            "am",
            "{}",
            ALLOC
                .os_allocator
                .get()
                .unwrap()
                .have
                .load(Ordering::SeqCst)
        );
        deb!(
            "{}",
            (ALLOC
                .os_allocator
                .get()
                .unwrap()
                .allocated
                .load(Ordering::SeqCst))
        );

        Ok(())
    }

    pub unsafe extern "C" fn main(&'static self, stack_top: u64, stack_len: u64) -> ! {
        {
            without_interrupts(|| {
                let mut lock = self.stack_data.write();
                lock.top = stack_top as *mut _;
                lock.len = stack_len as usize;
            });

            #[cfg(feature = "enable_stack_checks")]
            self.enable_stack_canaria();

            MAIN_PTR.call_once(|| self);

            log_info!("kernel", "thread safe", "creating gs...");

            let idt_stack = unsafe {
                alloc::alloc::alloc(Layout::from_size_align(stack_len as usize, 16).unwrap())
            };

            assert!(!idt_stack.is_null());

            unsafe { mem::thread_safe::init_gs(null_mut(), idt_stack.add(stack_len as usize)) };

            log_info!("kernel", "thread safe", "created gs");
        }

        let res = self.frist_init();

        #[cfg(feature = "enable_essential_safety_checks")]
        {
            log_info!("kernel", "main", "checking results");

            for (i, ret) in res.iter().enumerate() {
                if !ret.is_err() {
                    continue;
                }

                if i == 3 || i == 2 {
                    ret.clone().expect("Failed to Required subjects (graphic)");
                } else if i == 1 {
                    log_warn!(
                        "kernel",
                        "security",
                        "failed to attach micro code: {}",
                        ret.clone().unwrap_err().to_string()
                    );
                } else {
                    log_warn!(
                        "kernel",
                        "kernel",
                        "any failed(number: {}): {}",
                        i,
                        ret.clone().unwrap_err().to_string()
                    );
                }
            }
        }
        #[cfg(not(feature = "enable_essential_safety_checks"))]
        let _ = res;

        self.do_fn.get().unwrap()();

        #[cfg(feature = "enable_boot_option")]
        let mode = {
            log_info!("kernel", "main", "getting boot mode");

            let mode = self.get_boot_mode().expect("Failed to get boot mode");

            if mode == BootMode::Reboot {
                log_info!("kernel", "kernel", "rebooting");
                log_custom!("s", "ds", "dis", "");
                uefi::runtime::reset(ResetType::COLD, Status::SUCCESS, None);
            }

            mode
        };

        self.do_fn.get().unwrap()();

        log_info!("kernel", "main", "starting loading screen...");

        without_interrupts(|| {
            self.display_manager
                .gop_data
                .write()
                .as_mut()
                .unwrap()
                .get_good_mode()
                .unwrap();
            self.display_manager
                .start_load_grap()
                .expect("failed to start load screen");
        });

        self.do_fn.get().unwrap()();

        #[cfg(feature = "enable_boot_option")]
        let (size, should_add_alloc) = {
            let size = {
                #[cfg(feature = "include_boot_option_to_memcheck")]
                if mode == BootMode::MemCheck {
                    Some(unsafe { NonZeroUsize::new_unchecked(50 * 1024 * 1024) })
                } else {
                    None
                }
                #[cfg(not(feature = "include_boot_option_to_memcheck"))]
                None
            };

            let should_add_alloc = {
                #[cfg(feature = "include_boot_option_to_memcheck")]
                {
                    size.is_none()
                }
                #[cfg(not(feature = "include_boot_option_to_memcheck"))]
                {
                    true
                }
            };

            (size, should_add_alloc)
        };
        #[cfg(not(feature = "enable_boot_option"))]
        let (size, should_add_alloc) = (None, true);

        log_info!("kernel", "main", "initing memory...");

        unsafe {
            self.memory_manager
                .init_memory(size)
                .expect("failed to init memory system.");

            if should_add_alloc {
                self.memory_manager
                    .add_alloc()
                    .expect("failed to init memory system.");
            }

            log_custom!(
                "s",
                "ds",
                "am",
                "{}",
                ALLOC
                    .os_allocator
                    .get()
                    .unwrap()
                    .have
                    .load(Ordering::SeqCst)
            );
        };

        #[cfg(feature = "enable_boot_option")]
        if !should_add_alloc {
            log_info!(
                "kernel",
                "main",
                "memory check required. starting memory check..."
            );
            self.memcheck();
        }

        log_info!("kernel", "main", "exiting uefi...");

        self.do_fn.get().unwrap()();

        self.exit_uefi().expect("failed to exit uefi.");

        loop {
            spin_loop();
        }
    }
}

mod _internal_init {
    use crate::cpu::utils;
    use crate::{DEBUG_PROTOCOL_VERSION, Main, cpu, io, log_custom, log_debug, log_info, util};
    use core::alloc::Layout;
    use core::ptr;
    use uefi::runtime;

    #[cfg(feature = "enable_lldb_debug")]
    use core::arch::asm;

    #[inline(always)]
    pub unsafe extern "C" fn init_dep() {
        #[cfg(feature = "enable_uart_outputs")]
        io::console::serial::init_serial();
        uefi::helpers::init().expect("Failed to init uefi helpers");
    }

    #[inline(always)]
    pub fn get_boot_entropy() -> usize {
        let mut entropy: usize = 0;

        if let Ok(mut rng_proto) = util::proto::open::<uefi::proto::rng::Rng>(None) {
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
        log_custom!("s", "ds", "v", "{}", "0");
        log_info!("debug", "build info", "{}", "0");
        log_custom!("s", "ds", "pv", "{}", DEBUG_PROTOCOL_VERSION);

        if cfg!(feature = "enable_debug_outputs") {
            log_debug!(
                "debug",
                "cpu vendor",
                "{}, 0x{:x}",
                unsafe { cpu::utils::get_vendor_name() },
                unsafe { utils::cpuid(cpu::utils::cpuid::common::PIAFB, None) }.eax
            );

            log_debug!("debug", "full os info", "{:?}", "0")
        }
    }

    #[inline(always)]
    pub unsafe extern "C" fn allocate(target: *mut u64) {
        let entropy = (get_boot_entropy() % 65536) & !0xf;
        let stack_size = 1024 * 64;
        let main_size = size_of::<Main>();
        let main_align = align_of::<Main>();

        let total_size = stack_size + entropy + main_size + main_align;
        let layout = Layout::from_size_align(total_size, 4096).unwrap();
        let allocated = unsafe { alloc::alloc::alloc_zeroed(layout) };

        if allocated.is_null() {
            panic!("Allocation failed");
        }

        let stack_top = unsafe { allocated.add(stack_size) as usize } & !0xf;

        let struct_addr = (stack_top + entropy + main_align) & !(main_align - 1);
        let struct_ptr = struct_addr as *mut Main;

        unsafe {
            ptr::write(struct_ptr, Main::default());
        }

        unsafe {
            *target.offset(0) = stack_top as u64;
            *target.offset(1) = struct_ptr as u64;
            *target.offset(2) = stack_size as u64;
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
                                    //     let mut rdx: *mut u64 = func.args._table

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
        "mov rcx, [rsp + 40]",      // rcx: &u64        = rsp + 40  // &Self
        "mov r8, [rsp + 48]",       // r8 : &u64        = rsp + 48  // stack_len

        "and rdx, -16",             // rdx: *mut u64 &= -16 // rdx &= !15  // 16-bytes align

        "lea rsp, [rdx - 32]",      // rsp: *mut u8     = (rdx - 32) as *mut _ // move to new stack and reserve 32-byte stack

        "jmp {main}",               // main(rcx as &Main, rdx, r8)  // main(rcx: &Main, rdx: u64, r8: u64) -> !
        "ud2",                      // unreachable!();
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

    util::logger::LOG_CAPACITY.store(0, Ordering::SeqCst);

    log_last!("kernel", "panic", "panic raised.");
    log_last!("kernel", "panic", "version: {:?}", "0");

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
