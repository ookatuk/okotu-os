#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![feature(core_float_math)]

const VERSION: &str = "1.0.0";

/// OSプロトコルバージョン.
const DEBUG_PROTOCOL_VERSION: &str = "1.0";


const LINE_SPACING: f32 = 1.5;
const ENABLE_LIGATURES: bool = true;


const ALLOW_RATIOS: &[(usize, usize)] = &[
    (21, 9),
    (32, 9),
    (16, 9),
    (16, 10),
    (4, 3),
    (3, 2),
    (5, 4),
];

unsafe extern "C" {
    static __ImageBase: u8;
}

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::arch::x86_64;
use core::panic::PanicInfo;
use core::ptr::NonNull;
use spin::RwLock;
use bitflags::bitflags;
use num_traits::Zero;
use spin::mutex::Mutex;
use uefi::boot::{OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::{entry, Identify};
use uefi_raw::Status;
use util::result;
use crate::util::result::Error;

extern crate alloc;

mod fonts;
mod cpu;
mod io;
mod rng;
mod fs;
mod util;

#[global_allocator]
/// 物理/仮想アロケーター.
pub static ALLOC: uefi::allocator::Allocator = uefi::allocator::Allocator;

bitflags!{
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
    pub fn new(ptr: *const fn(&Main) -> result::Result, level: u8) -> Self {
        Self {
            ptr: (ptr.addr() ^ level as usize ^ 7) as u64,
            reversed_ptr: ptr.addr().reverse_bits() as u64,
            level,
            state: State::empty()
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

#[derive(Default, Debug)]
struct Main<'a> {
    gop_data: RwLock<Option<&'a mut io::console::gop::GopData>>,
    watching_list: &'a [Mutex<WatchingItem>],
}


impl Main<'_> {
    fn init_dep(&self) {
        uefi::helpers::init().expect("Failed to init uefi helpers");
    }

    fn init_gop(&self) -> result::Result<()> {
        let handle = Error::try_raise(uefi::boot::
        locate_handle_buffer(SearchType::ByProtocol(&uefi::proto::console::gop::GraphicsOutput::GUID)), Some("Failed to get GOP handle"))?;

        let mut gop = unsafe {
            Error::try_raise(uefi::boot::open_protocol::<uefi::proto::console::gop::GraphicsOutput>(
                OpenProtocolParams {
                    handle: handle[0],              // 取得したハンドル
                    agent: uefi::boot::image_handle(), // 自分のImageHandle
                    controller: None,               // 基本はNoneでOK
                },
                OpenProtocolAttributes::GetProtocol,
            ), Some("failed to open GP protocol"))?
        };

        // いい感じのを選ぶ
        // w, レベル, index
        let mut target: Option<(usize, usize, uefi::proto::console::gop::Mode)> = None;

        for mode in gop.modes() {
            let info = mode.info();
            let (w, h) = info.resolution();

            if let Some((level, _)) = ALLOW_RATIOS.iter().enumerate().find(|&(_, &(rw, rh))| w * rh == h * rw) {

                let is_better = if let Some((best_w, best_level, _)) = target {
                    // レベルが低い（優先度が高い）か、同じレベルで幅が広い場合
                    level < best_level || (level == best_level && w > best_w)
                } else {
                    true
                };

                if is_better {
                    target = Some((w, level, mode));
                }
            }
        }

        if let Some((_, _, mode)) = target {
            Error::try_raise(gop.set_mode(&mode), Some("Failed to set video mode"))?;
        }

        let info = gop.current_mode_info();
        let (w, h) = info.resolution();

        let fb_addr = gop.frame_buffer().as_mut_ptr() as *mut u32;

        let gop_data = Box::leak(Box::new(io::console::gop::GopData{
            ptr: NonNull::new(fb_addr).unwrap(),
            w,
            h,
            stride: info.stride(),
        }));

        let mut data = self.gop_data.write();

        if let Some(old_ref) = data.take() {
            unsafe {
                let _ = Box::from_raw(old_ref as *mut _);
            }
        }

        *data = Some(gop_data);

        Ok(())
    }

    fn frist_init(&self) -> Vec<result::Result> {
        let mut ret = vec![];

        self.init_dep();
        ret.push(cpu::mitigation::ucode::load());
        
        ret.push(self.init_gop());

        ret
    }

    fn a_run_watching(&self, is_bsp: bool) -> u8 { //! TODO (同権限内の)Spectre及びBHIの大部分の系列, Rowhammer脆弱性の踏み台になる可能性の対策
        for (_, i) in self.watching_list.iter().enumerate() {
            let mut data = i.lock();

            unsafe{x86_64::_mm_lfence()};
            if data.is_none() || data.is_running() || (data.is_dest() && !is_bsp) {
                core::hint::spin_loop();
                continue;
            }

            unsafe{x86_64::_mm_lfence()}; // BHIとかSpectreやRowhammer対策
            let de_ptr = data.ptr ^ data.level as u64 ^ 7;

            unsafe{x86_64::_mm_lfence()};
            if de_ptr as u64 != (data.reversed_ptr.reverse_bits()) {
                data.state |= State::ERR;  // ぶっこわれてるのであうと
                return 2;  // 何なら攻撃の可能性もある
            }

            // 軽減策終了

            data.state |= State::RUNNING;

            let func = unsafe { core::mem::transmute::<*const (), fn(&Main) -> result::Result>(de_ptr as *const _) };
            let result = func(self);

            data.state &= !State::RUNNING;
            data.ptr = 0;

            if result.is_err() {
                data.state |= State::ERR;
                return 1;
            }
            return 0;
        }
        0
    }

    pub fn main(&self) -> ! {
        let res = self.frist_init();
        if res[1].is_err() {
            res[1].clone().expect("Failed to get GOP data");
        }

        loop {
            core::hint::spin_loop();
        }
    }
}

#[entry]
pub fn main() -> Status {
    let main = Main::default();

    main.main();
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop()
    }
}

#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {  // アロケーターエラー関係
    panic!("alloc failed: {:?}", layout);
}
