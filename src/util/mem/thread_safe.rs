use alloc::boxed::Box;
use core::arch::asm;
use core::ops::{Deref, DerefMut};
use core::ptr::{addr_of, null_mut};
use crate::cpu;

#[repr(Rust)]
#[derive(Debug, Default)]
pub struct GsMainData {

}

#[repr(C)]
#[derive(Debug, Default)]
pub struct Gs {
    pub app_stack: u64,
    pub kernel_stack: u64,
    main_data: GsMainData,  // カプセル化
}

impl Deref for Gs {
    type Target = GsMainData;
    
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.main_data
    }
}

impl DerefMut for Gs {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.main_data
    }
}


#[inline]
pub fn get_mut() -> Option<&'static mut Gs> {
    let value: u64;
    unsafe {
        asm!("mov {}, gs:", out(reg) value, options(nostack, readonly, preserves_flags));
        (value as *mut Gs).as_mut()
    }
}

pub unsafe fn new_kernel_side_gs(app_stack: Option<*const u8>, kernel_stack: Option<*const u8>) {
    let gs = Box::leak(Box::new(Gs {
        app_stack: app_stack.unwrap_or(null_mut()).addr() as u64,
        kernel_stack: kernel_stack.unwrap_or(null_mut()).addr() as u64,
        main_data: Default::default(),
    }));

    unsafe{cpu::utils::write_msr(
        cpu::utils::msr::common::GS_BASE,
        addr_of!(gs).addr() as u64
    )};
}