use core::alloc::Layout;
use core::ptr::NonNull;
#[repr(C, align(4096))]
pub struct InternalSlab {
    bitmap: [u64; 2],
    slot_size: u32,
    pub(crate) next: Option<NonNull<InternalSlab>>,
    _padding: [u8; 8],
    data: [u8; 4064],
}

impl InternalSlab {
    pub unsafe fn init_at(ptr: *mut u8, slot_size: u32, next: Option<NonNull<InternalSlab>>) -> &'static mut Self {
        let slab = unsafe{&mut *(ptr as *mut Self)};
        slab.bitmap = [0, 0];
        slab.slot_size = slot_size;
        slab.next = next;
        slab
    }

    pub unsafe fn alloc(&mut self, layout: Layout) -> Option<*mut u8> {
        let size = layout.size();
        if size > self.slot_size as usize { return None; }

        for (i, map) in self.bitmap.iter_mut().enumerate() {
            let first_free = (!*map).trailing_zeros();
            if first_free < 64 {
                let slot_idx = (i * 64) + first_free as usize;
                let offset = 32 + (slot_idx * self.slot_size as usize);
                if offset + size <= 4096 {
                    *map |= 1 << first_free;
                    return unsafe{Some((self as *mut Self as *mut u8).add(offset))};
                }
            }
        }
        None
    }

    pub unsafe fn dealloc(&mut self, ptr: *mut u8) {
        let offset = (ptr as usize) - (self as *mut Self as usize);
        let slot_idx = (offset - 32) / self.slot_size as usize;
        self.bitmap[slot_idx >> 6] &= !(1 << (slot_idx & 63));
    }
}