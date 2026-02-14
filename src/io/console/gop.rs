use core::ptr::NonNull;

#[derive(Debug)]
pub struct GopData {
    pub ptr: NonNull<u32>,
    pub w: usize,
    pub h: usize,
    pub stride: usize,
}

impl GopData {
    #[inline]
    pub unsafe fn draw_pixel(&self, x: usize, y: usize, color: u32) {
        if x < self.w && y < self.h {
            return self.draw_pixel_unchecked(x, y, color);
        }
    }

    #[inline]
    pub unsafe fn draw_pixel_unchecked(&self, x: usize, y: usize, color: u32) {
        let offset = y * self.stride + x;
        unsafe { self.ptr.add(offset).write_volatile(color) };
    }

    pub unsafe fn clear(&self, color: u32) {
        let color64 = ((color as u64) << 32) | (color as u64);
        let ptr = self.ptr.as_ptr();

        if self.w == self.stride {
            let count = (self.w * self.h) / 2;
            core::arch::asm!(
            "rep stosq",
            inout("rcx") count => _,
            inout("rdi") ptr => _,
            in("rax") color64,
            options(nostack, preserves_flags)
            );
        } else {
            for y in 0..self.h {
                let row_ptr = ptr.add(y * self.stride);
                let count = self.w / 2;
                core::arch::asm!(
                "rep stosq",
                inout("rcx") count => _,
                inout("rdi") row_ptr => _,
                in("rax") color64,
                options(nostack, preserves_flags)
                );
            }
        }
    }
}

unsafe impl Send for GopData {}
unsafe impl Sync for GopData {}
