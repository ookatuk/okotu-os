use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use core::hint::{spin_loop};
use core::ops::{Deref, DerefMut};
use x86_64::structures::idt::InterruptDescriptorTable;
use crate::ALLOC;
use crate::asy_nc::{CoreExecutor, Executor};
use crate::cpu::msr;
use crate::interrupt::raw::IdtRawStacks;
use crate::memory::paging::TopPageTable;
use crate::timer::tsc::TscGsData;

#[derive(Default)]
#[repr(C)]
pub struct ThreadLocalStorage<'a> {
    self_ptr: *mut ThreadLocalStorage<'a>,
    inner: ThreadLocalStorageInner<'a>,
}

#[derive(Default)]
pub struct ThreadLocalStorageInner<'a> {
    pub cpu_id: u32,
    pub page_table: TopPageTable,
    pub internal_cpu_flag_cache: crate::cpu_flags::CpuFlagCache,
    pub tsc_data: TscGsData,
    pub tsc_init: bool,
    pub idt_raw: InterruptDescriptorTable,
    pub idt_stack: IdtRawStacks<'a>,
    pub executor: CoreExecutor
}


pub unsafe fn write_none() {
    let layout = Layout::new::<ThreadLocalStorage>();

    let ptr = unsafe {
        let raw = ALLOC.alloc(layout) as *mut ThreadLocalStorage;
        if raw.is_null() {
            loop {
                spin_loop();
            }
        }

        raw.write(ThreadLocalStorage::default());

        &mut *raw
    };

    ptr.self_ptr = ptr as *mut ThreadLocalStorage;

    unsafe{msr::write(
        msr::msr_address::IA32_GS_BASE,
        ptr.self_ptr as u64
    )};
}

pub fn read_gs() -> Option<&'static mut ThreadLocalStorage<'static>> {
    let value: *mut ThreadLocalStorage;
    unsafe {
        asm!(
            "mov {0}, gs:[0]",
            out(reg) value,
            options(nostack, preserves_flags, readonly)
        );
    }

    Some(unsafe{&mut *value})
}

impl<'a> Deref for ThreadLocalStorage<'a> {
    type Target = ThreadLocalStorageInner<'a>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ThreadLocalStorage<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}