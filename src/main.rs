#![no_std]
#![no_main]

extern crate alloc;
const VERSION: &str = "1.0.0";

/// OSプロトコルバージョン.
const DEBUG_PROTOCOL_VERSION: &str = "1.0";

const ENABLE_DEBUG: bool = true;

const LINE_SPACING: f32 = 1.5;

const ENABLE_LIGATURES: bool = true;

const MAX_DO_ITEM: usize = 1000;

const BAR_HEIGHT: usize = 20;
const BAR_MARGIN: usize = 50;

const GUI_WAIT: usize = 2_000_000;

const PANICED_TO_RESTART_TIME: usize = 20;

const ALLOW_RATIOS: &[(usize, usize)] =
    &[(21, 9), (32, 9), (16, 9), (16, 10), (4, 3), (3, 2), (5, 4)];

const MAIN_FONT: &'static [u8] = include_bytes!("../assets/ZeroveItalic.ttf");

unsafe extern "C" {
    static __ImageBase: u8;
}

use crate::manager::display_manager::DisplayManager;
use crate::manager::load_task_manager::LoadTaskManager;
use crate::manager::memory_manager::MemoryManager;
use crate::util::mem::allocator::main::OsAllocator;
use crate::util::result::Error;
use crate::util::timer::TSC;
use acpi;
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use bitflags::bitflags;
use core::alloc::Layout;
use core::arch::{asm, naked_asm};
use core::ffi::c_void;
use core::hint::spin_loop;
use core::panic::PanicInfo;
use core::ptr::{NonNull, addr_of, null_mut};
use core::time::Duration;
use fontdue::Font;
use num_traits::Zero;
use serde::Deserialize;
use spin::{Once, RwLock};
use uefi::boot::{TimerTrigger, set_image_handle};
use uefi::table::set_system_table;
use uefi::{Event, boot, entry};
use uefi_raw::Status;
use uefi_raw::table::boot::{EventType, Tpl};
use uefi_raw::table::system::SystemTable;
use util::result;
use x86_64::instructions::interrupts;
use x86_64::instructions::interrupts::without_interrupts;

mod cpu;
mod fonts;
mod fs;
mod io;
mod manager;
mod rng;
mod util;

#[global_allocator]
/// 物理/仮想アロケーター.
pub static ALLOC: OsAllocator = OsAllocator::new();

bitflags! {
    #[derive(Debug)]
    pub struct State: u8 {
        const DEST = 1 << 0;
        const RUNNING = 1 << 1;
        const ERR = 1 << 2;
    }
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

#[derive(Default)]
#[repr(align(16))]
struct Main {
    global_font: Arc<RwLock<Option<Font>>>,
    tsc_timer: Arc<RwLock<TSC>>,

    stack_data: RwLock<StackData>,

    do_fn: Once<Arc<dyn Fn()>>,

    load_task_manager: Arc<LoadTaskManager>,

    display_manager: DisplayManager,
    memory_manager: MemoryManager,
}

impl Main {
    fn init_font(&self) -> result::Result<()> {
        let new_font = Box::new(fonts::load_font(MAIN_FONT));
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

        ret.push(|| -> result::Result {
            interrupts::without_interrupts(|| {
                self.tsc_timer.write().init(None)?;
                util::logger::init_timer(self.tsc_timer.clone());
                Ok(())
            })
        }());

        ret.push(cpu::mitigation::ucode::load());

        ret.push(self.init_font());

        ret.push(self.display_manager.init_gop());

        ret.push(self.display_manager.start_load_grap());

        ret
    }

    pub extern "efiapi" fn check_canaria(_event: uefi::Event, context: Option<NonNull<c_void>>) {
        let me = unsafe { &*(context.unwrap().as_ptr() as *const Main) };

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

    pub unsafe extern "C" fn main(&self, stack_top: u64, stack_len: u64) -> ! {
        {
            without_interrupts(|| {
                let mut lock = self.stack_data.write();
                lock.top = stack_top as *mut _;
                lock.len = stack_len as usize;
            });

            self.enable_stack_canaria();

            log_info!("kernel", "thread safe", "creating gs...");

            let idt_stack =
                alloc::alloc::alloc(Layout::from_size_align(stack_len as usize, 16).unwrap());

            assert!(!idt_stack.is_null());

            unsafe {
                util::mem::thread_safe::init_gs(null_mut(), idt_stack.add(stack_len as usize))
            };

            log_info!("kernel", "thread safe", "created gs");
        }

        let res = self.frist_init();

        log_info!("kernel", "main", "checking results");

        for (i, ret) in res.iter().enumerate() {
            if !ret.is_err() {
                continue;
            }

            if i == 3 {
                ret.clone().expect("Failed to get GOP data");
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

        self.do_fn.get().unwrap()();

        self.memory_manager
            .init_memory()
            .expect("failed to init memory system.");

        log_info!("kernel", "main", "exiting uefi...");

        self.do_fn.get().unwrap()();
        {
            let event = self
                .display_manager
                .gop_uefi_event
                .get()
                .unwrap()
                .unsafe_clone();

            let _ = boot::close_event(event).expect("failed to close grap event");

            let _ = boot::exit_boot_services(None);
        }

        self.display_manager.do_load_grap_in_now();

        loop {
            spin_loop();
        }
    }
}

mod _internal_init {
    use crate::cpu::utils;
    use crate::{
        DEBUG_PROTOCOL_VERSION, ENABLE_DEBUG, Main, VERSION, cpu, io, log_custom, log_debug, util,
    };
    use core::alloc::Layout;
    use core::ptr;
    use uefi::runtime;

    #[inline(always)]
    pub unsafe extern "C" fn init_dep() {
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
        log_custom!("s", "ds", "a", "");
        log_custom!("s", "ds", "d", "{}", if ENABLE_DEBUG { 1 } else { 0 });
        log_custom!("s", "ds", "v", "{}", VERSION);
        log_custom!("s", "ds", "pv", "{}", DEBUG_PROTOCOL_VERSION);

        if ENABLE_DEBUG {
            log_debug!(
                "debug",
                "cpu vendor",
                "{}, 0x{:x}",
                unsafe { cpu::utils::get_vendor_name() },
                unsafe { utils::cpuid(cpu::utils::cpuid::common::PIAFB, None) }.eax
            );
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
#[unsafe(no_mangle)]
#[unsafe(export_name = "efi_main")]
pub extern "efiapi" fn efi_main(_head: uefi::Handle, _table: *const c_void) -> ! {
    naked_asm!(
        "endbr64",                  // for CFG (Control Flow Guard) instructions

                                    // let rbx: *const u64;
                                    // let r12: *const u64;
                                    // let rdx: *const u64;

                                    // let mut rcx: *const u64 = _head .addr()
                                    // let mut rdx: *const u64 = _table.addr()

                                    // let mut rsp: *mut u8    = get_stack_pointer!().addr()
                                    // let mut gs : *const u16 = get_gs_register!().addr()

        "xor rbx, rbx",             // rbx: *const u64  = 0
        "mov gs, bx",               // gs : *const u16  = rbx as u16

        "sub rsp, 56",              // rsp: *mut u8    -= 56  // reserve 56-byte stack frame above current rsp

        "mov r12, rdx",             // r12: *const u64  = rdx
        "call {set_handle}",        // set_handle(rcx) // rcx to ?

        "mov rcx, r12",             // rcx: *const u64  = r12
        "call {set_table}",         // set_table(rcx)  // rcx to ?

        "call {init_dep}",          // init_dep()

        "call {debug_hand_shake}",  // call_hand_shake()

        "lea rcx, [rsp + 32]",      // rcx: *const u64  = rsp.add(32) as u64

        "call {allocate}",          // allocate(rcx)  // rcx to ?
                                    // rcx.add(0)       = u64  // stack_top
                                    // rcx.add(1)       = u64  // *mut Main
                                    // rcx.add(2)       = u64  // stack_len

        "mov rdx, [rsp + 32]",      // rdx: &u64        = rsp.add(32)  // stack_top
        "mov rcx, [rsp + 40]",      // rcx: &u64        = rsp.add(40)  // &Self
        "mov r8, [rsp + 48]",       // r8 : &u64        = rsp.add(48)  // stack_len

        "and rdx, -16",             // rdx: *const u64 &= -16  // 16-bytes align

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

    log_last!("kernel", "panic", "{}\n{}", loc, message);
    log_last!(
        "kernel",
        "panic",
        "A critical system error has occurred. System will restart in {} seconds. for system admin: (info: {}, by: {})",
        PANICED_TO_RESTART_TIME,
        info.message(),
        info.location().unwrap()
    );

    loop {
        spin_loop()
    }
}
