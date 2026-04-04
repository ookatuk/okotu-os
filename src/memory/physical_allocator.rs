use core::alloc::{GlobalAlloc, Layout};
use spin::Once;
use spinning_top::RawSpinlock;
use x86_64::instructions::interrupts::without_interrupts;
use crate::{ALLOCATOR_ADD_OFFSET, POSITION_VALUE};
use crate::util_types::CanRangeData;

pub struct OsPhysicalAllocator {
    pub uefi_alloc: uefi::allocator::Allocator,
    pub os_allocator: talc::TalcLock<RawSpinlock, talc::source::Manual>,
    pub use_os_alloc: Once,
}

impl OsPhysicalAllocator {
    #[inline]
    pub const fn new() -> Self {
        Self {
            uefi_alloc: uefi::allocator::Allocator{},
            os_allocator: talc::TalcLock::new(talc::source::Manual),
            use_os_alloc: Once::new(),
        }
    }

    pub unsafe fn add_target_to_os_alloc(&self, data: crate::util_types::MemRangeData<usize>) {
        let mut ptr = data.start() as *mut u8;
        let mut len = data.len();

        if ptr.addr() < ALLOCATOR_ADD_OFFSET {
            let tmp = ALLOCATOR_ADD_OFFSET - ptr.addr();
            ptr = ALLOCATOR_ADD_OFFSET as *mut u8;

            if len < tmp || len - tmp < 1024 {
                return;
            }

            len -= tmp;
        }

        without_interrupts(|| { unsafe{
            let mut lock = self.os_allocator.lock();

            lock.claim(
                ptr,
                len,
            );
        }});
    }

    pub unsafe fn change_to_os_allocator(&self) {
        self.use_os_alloc.call_once(|| {});
    }
}

unsafe impl GlobalAlloc for OsPhysicalAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            if self.use_os_alloc.is_completed() {
                self.os_allocator.alloc(layout)
            } else {
                self.uefi_alloc.alloc(layout)
            }
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            if self.use_os_alloc.is_completed() {
                self.os_allocator.dealloc(ptr, layout)
            } else {
                self.uefi_alloc.dealloc(ptr, layout)
            }
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe {
            if self.use_os_alloc.is_completed() {
                self.os_allocator.alloc_zeroed(layout)
            } else {
                self.uefi_alloc.alloc_zeroed(layout)
            }
        }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe {
            if self.use_os_alloc.is_completed() {
                self.os_allocator.realloc(ptr, layout, new_size)
            } else {
                self.uefi_alloc.realloc(ptr, layout, new_size)
            }
        }
    }
}