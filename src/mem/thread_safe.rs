use crate::cpu;
use crate::mem::paging::types::TopPageTable;
use alloc::boxed::Box;
use core::arch::asm;
use core::ops::{Deref, DerefMut};
use rhai::CustomType;
use rhai::TypeBuilder;

#[repr(Rust)]
#[derive(Debug, Default, Clone)]
pub struct GsMainDataNotCustom {
    pub page_table: Option<TopPageTable>,
}

impl CustomType for GsMainDataNotCustom {
    fn build(_: TypeBuilder<Self>) {}
}

#[repr(Rust)]
#[derive(Debug, Default, Clone, CustomType)]
pub struct GsMainData {
    pub cpu_id: u32,
    pub sub: GsMainDataNotCustom,
}

impl Deref for GsMainData {
    type Target = GsMainDataNotCustom;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.sub
    }
}

impl DerefMut for GsMainData {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sub
    }
}

#[repr(C, align(16))]
#[derive(Debug, Default, Clone, CustomType)]
pub struct Gs {
    pub self_ptr: u64,
    pub app_stack: u64,
    pub kernel_stack: u64,
    main_data: GsMainData, // カプセル化
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
    let mut ptr: *mut Gs = core::ptr::null_mut();
    unsafe {
        asm!(
            "mov {tmp:e}, gs",
            "test {tmp:e}, {tmp:e}",
        "jz 2f",
            "mov {tmp}, gs:[0]",
        "2:",
            "endbr64",
            tmp = inout(reg) ptr,
            options(nostack, readonly, preserves_flags)
        );
        ptr.as_mut()
    }
}
pub unsafe fn init_gs(app_stack: *const u8, kernel_stack: *const u8) {
    let gs = Box::new(Gs {
        self_ptr: 0, // 後で入れる
        app_stack: app_stack as u64,
        kernel_stack: kernel_stack as u64,
        main_data: Default::default(),
    });

    let ptr = Box::leak(gs);
    ptr.self_ptr = (ptr as *mut Gs).addr() as u64;

    unsafe {
        asm!(
        "mov {tmp:e}, 0x08",
        "mov gs, {tmp:e}",
        tmp = out(reg) _,
        options(nostack, preserves_flags, nomem)
        );
    }

    unsafe {
        cpu::utils::write_msr(
            cpu::utils::msr::common::GS_BASE,
            (ptr as *const Gs).addr() as u64,
        )
    };
}
