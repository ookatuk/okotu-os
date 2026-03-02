use core::alloc::{GlobalAlloc, Layout};
use spin::Mutex;

pub struct LockedAllocator(Mutex<uefi::allocator::Allocator>);

impl LockedAllocator {
    pub const fn new() -> Self {
        LockedAllocator(Mutex::new(uefi::allocator::Allocator))
    }
}

unsafe impl GlobalAlloc for LockedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe{self.0.lock().alloc(layout)}
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe{self.0.lock().dealloc(ptr, layout)}
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe{self.0.lock().alloc_zeroed(layout)}
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe{self.0.lock().realloc(ptr, layout, new_size)}
    }
}