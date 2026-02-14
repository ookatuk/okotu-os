use alloc::vec;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{NonNull, null_mut};
use crate::util::mem::allocator::frame_boundary_tag::BoundaryTagFrameAllocator;
use crate::util::mem::allocator::slab::InternalSlab;
use crate::util::mem::types::MemData;

pub struct HybridAllocatorInner {
    slab_heads: Vec<Option<NonNull<InternalSlab>>>,
    frame_allocs: Vec<BoundaryTagFrameAllocator>,
}

pub struct HybridAllocator(spin::Mutex<HybridAllocatorInner>);

impl HybridAllocator {
    pub fn new(data_list: Vec<MemData<usize>>) -> Self {
        let mut frame_allocs = Vec::new();

        for data in data_list {
            if let Ok((_rem, alloc)) = BoundaryTagFrameAllocator::new(data) {
                frame_allocs.push(alloc);
            }
        }

        let slab_heads = vec![None; 9];

        Self(spin::Mutex::new(HybridAllocatorInner {
            slab_heads,
            frame_allocs,
        }))
    }
}


impl HybridAllocatorInner {
    /// 複数の卸売業者（FrameAllocator）から空きを探す「はしご」ロジック
    unsafe fn alloc_from_frames(&mut self, layout: Layout) -> *mut u8 {
        for alloc in &mut self.frame_allocs {
            let ptr = unsafe{alloc.alloc(layout)};
            if !ptr.is_null() {
                return ptr;
            }
        }
        null_mut()
    }

    /// どのアロケータの管轄か判定して返却（絶対アドレスを使用）
    unsafe fn dealloc_from_frames(&mut self, ptr: *mut u8, layout: Layout) {
        let addr = ptr as usize;

        for alloc_mutex in &mut self.frame_allocs {
            let mut alloc = alloc_mutex.0.lock();

            let start_page = unsafe{alloc.table_ptr.start.as_ptr().read() as usize};
            let start_addr = start_page << 12;

            let manage_limit = start_addr + (u32::MAX as usize);

            if addr >= start_addr && addr < manage_limit {
                unsafe{alloc.dealloc(ptr, layout)};
                return;
            }
        }
    }
}

unsafe impl GlobalAlloc for HybridAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut mgr = self.0.lock();
        let size = layout.size();

        if size > 2048 || layout.align() > 4096 {
            return unsafe{mgr.alloc_from_frames(layout)};
        }

        let bucket = (size.next_power_of_two().max(8).trailing_zeros() - 3) as usize;
        let slot_size = 1 << (bucket + 3);

        // 1. 既存スラブに空きがあるか探索
        // Vecの中身を Option から取り出してループ
        if let Some(current) = mgr.slab_heads[bucket] {
            let mut curr_ptr = Some(current);
            while let Some(mut slab_ptr) = curr_ptr {
                let slab = unsafe{slab_ptr.as_mut()};
                if let Some(ptr) = unsafe{slab.alloc(layout)} {
                    return ptr;
                }
                curr_ptr = slab.next;
            }
        }

        // 2. スラブ満杯なら、どこか1つの業者から4KB卸してもらう
        let new_page = unsafe{mgr.alloc_from_frames(Layout::from_size_align_unchecked(4096, 4096))};
        if new_page.is_null() { return null_mut(); }

        // 3. 新しいスラブを初期化し、リストの先頭に挿入
        // 以前の先頭(mgr.slab_heads[bucket])を next に渡す
        let new_slab = unsafe{InternalSlab::init_at(new_page, slot_size as u32, mgr.slab_heads[bucket])};
        let new_slab_ptr = unsafe{NonNull::new_unchecked(new_slab)};

        // Vec の中身を更新
        mgr.slab_heads[bucket] = Some(new_slab_ptr);

        unsafe{new_slab.alloc(layout).unwrap_or(null_mut())}
    }


    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut mgr = self.0.lock();
        if layout.size() > 2048 || layout.align() > 4096 {
            unsafe{mgr.dealloc_from_frames(ptr, layout)};
        } else {
            let slab_addr = (ptr as usize) & !4095;
            let slab = unsafe{&mut *(slab_addr as *mut InternalSlab)};
            unsafe{slab.dealloc(ptr)};
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();

        if size > 2048 || layout.align() > 4096 {
            let mut mgr = self.0.lock();
            for alloc_mutex in &mut mgr.frame_allocs {
                let ptr = unsafe{alloc_mutex.0.lock().alloc_zeroed(layout)};
                if !ptr.is_null() { return ptr; }
            }
            return null_mut();
        }

        let ptr = unsafe{self.alloc(layout)};
        if !ptr.is_null() {
            unsafe{core::ptr::write_bytes(ptr, 0, size)};
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old_size = layout.size();

        if old_size > 2048 && new_size > 2048 {
            let mut mgr = self.0.lock();
            let addr = ptr as usize;
            for alloc_mutex in &mut mgr.frame_allocs {
                let mut alloc = alloc_mutex.0.lock();
                let start_page = unsafe{alloc.table_ptr.start.as_ptr().read()} as usize;
                if addr >= (start_page << 12) && addr < (start_page << 12) + (u32::MAX as usize) {
                    return unsafe{alloc.realloc(ptr, layout, new_size)};
                }
            }
        }
        unsafe {
            let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
            let new_ptr = self.alloc(new_layout);
            if !new_ptr.is_null() {
                core::ptr::copy_nonoverlapping(ptr, new_ptr, old_size.min(new_size));
                self.dealloc(ptr, layout);
            }
            new_ptr
        }
    }
}