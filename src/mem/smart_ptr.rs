use core::alloc::Layout;
use core::ops::{Deref, DerefMut};

pub struct RangePtr {
    ptr: *mut u8,
    layout: Layout,
}

impl RangePtr {
    pub unsafe fn new(ptr: *mut u8, layout: Layout) -> Self {
        Self { ptr, layout}
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.layout.size()) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }

    pub unsafe fn leak(self) -> &'static mut [u8] {
        let ptr = self.ptr;
        let size = self.layout.size();

        core::mem::forget(self);

        unsafe { core::slice::from_raw_parts_mut(ptr, size) }
    }
}

impl Deref for RangePtr {
    type Target = [u8];
    fn deref(&self) -> &Self::Target { self.as_slice() }
}

impl DerefMut for RangePtr {
    fn deref_mut(&mut self) -> &mut Self::Target { self.as_mut_slice() }
}

impl Drop for RangePtr {
    fn drop(&mut self) {
        unsafe {
            alloc::alloc::dealloc(self.ptr, self.layout);
        }
    }
}