use crate::{log_info, log_warn, POSITION_VALUE};
use crate::mem::types::{MemData, MemMap};
use crate::util::result::{Error, ErrorType};
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{NonNull, null_mut};

#[inline]
const fn remove<T>(target: *mut T, index: usize, max: usize) {
    if index >= max {
        return;
    }
    if index == max - 1 {
        return;
    }
    unsafe {
        let ptr = target.add(index);
        core::ptr::copy(ptr.add(1), ptr, max - index - 1);
    }
}

#[inline]
const fn insert<T>(target: *mut T, index: usize, value: T, max: usize) {
    unsafe {
        let ptr = target.add(index);
        core::ptr::copy(ptr, ptr.add(1), max - index);
        ptr.write(value);
    }
}

pub struct InternalBoundaryTagFrameAllocator {
    pub table_ptr: MemMap<NonNull<u32>>,
    table_len: u32,
}

impl InternalBoundaryTagFrameAllocator {
    pub const fn new(arg_target: MemData<usize>) -> Result<(MemData<usize>, Self), Error> {
        if (arg_target.start & 0xFFF) != 0 || arg_target.len < 8192 {
            return Err(Error::new(
                ErrorType::InvalidData,
                Some("Invalid alignment or size"),
            ));
        }

        let len_shifted = arg_target.len >> 12;
        let u32_max = u32::MAX as usize;
        let manage_pages = if len_shifted < u32_max {
            len_shifted
        } else {
            u32_max
        };
        let manage_len = manage_pages << 12;

        let table_start_addr = arg_target.start + manage_len - 4096;
        let table_ptr = unsafe { NonNull::new_unchecked(table_start_addr as *mut u32) };

        let data_start_page_idx = (arg_target.start >> 12) as u32;
        let data_page_count = (manage_pages - 1) as u32;

        unsafe {
            #[cfg(all(feature = "enable_memory_poisoning", feature = "enable_overprotective_safety_checks"))]
            core::ptr::write_bytes(
                arg_target.start as *mut u8,
                POSITION_VALUE,
                manage_len
            );

            #[cfg(all(feature = "enable_memory_poisoning", not(feature = "enable_overprotective_safety_checks")))]
            unsafe { (arg_target.start as *mut u32).add(1).write(0x2F2F2F2F); }

            (arg_target.start as *mut u32).write(data_page_count);

            let data_end_tag_addr = table_start_addr - size_of::<u32>();
            (data_end_tag_addr as *mut u32).write(data_page_count);

            table_ptr.as_ptr().write(data_start_page_idx);
        }

        let remaining_mem = MemData {
            start: arg_target.start + manage_len,
            len: arg_target.len - manage_len,
        };

        // タプルでそのまま返す
        Ok((
            remaining_mem,
            Self {
                table_ptr: MemMap {
                    start: table_ptr,
                    end: unsafe { table_ptr.add(4096 / core::mem::size_of::<u32>()) },
                },
                table_len: 1, // 既に上で1件書き込み済み
            },
        ))
    }

    const fn is_allocated(&self, addr: usize) -> bool {
        let target_page = (addr >> 12) as u32;

        let mut i = 0;
        while i < self.table_len {
            let entry_ptr = unsafe { self.table_ptr.start.as_ptr().add(i as usize) };
            let start_page = unsafe { entry_ptr.read() };

            let size = unsafe { (((start_page as usize) << 12) as *const u32).read() };

            if target_page >= start_page && target_page < (start_page + size) {
                return false;
            }

            i += 1;
        }

        true
    }

    const fn is_full(&self) -> bool {
        let max_entries = unsafe {
            self.table_ptr
                .end
                .as_ptr()
                .offset_from(self.table_ptr.start.as_ptr())
        } as usize;

        self.table_len as usize >= max_entries
    }

    fn try_add_table_map(&mut self) -> Result<(), Error> {
        if !self.is_full() {
            return Ok(());
        }

        let table_end_page = (self.table_ptr.end.as_ptr() as usize) >> 12;

        for i in 0..self.table_len {
            let entry_ptr = unsafe { self.table_ptr.start.as_ptr().add(i as usize) };
            let loc: usize = unsafe { entry_ptr.read() } as usize; // 空きブロックの開始ページ
            let size = unsafe { ((loc << 12) as *const u32).read() } as usize;

            if table_end_page >= loc && table_end_page < (loc + size) {
                unsafe {
                    if size == 1 {
                        remove(
                            self.table_ptr.start.as_ptr(),
                            i as usize,
                            self.table_len as usize,
                        );
                        self.table_len -= 1;
                    } else if loc == table_end_page {
                        let new_loc = (loc + 1) as u32;
                        entry_ptr.write(new_loc);
                        (((new_loc as usize) << 12) as *mut u32).write((size - 1) as u32);
                    } else {
                        ((loc << 12) as *mut u32).write((size - 1) as u32);
                    }

                    self.table_ptr.end = NonNull::new_unchecked(
                        self.table_ptr.end.as_ptr().add(4096 / size_of::<u32>()),
                    );
                }
                return Ok(());
            }
        }

        Err(Error::new(
            ErrorType::AllocationFailed,
            Some("Table full: No adjacent free page found"),
        ))
    }

    pub fn dump_table(&self) {
        log_info!(
            "kernel",
            "dump",
            "--- Allocator Table Dump (len: {}) ---",
            self.table_len
        );

        for i in 0..self.table_len {
            unsafe {
                // 管理簿のエントリ（開始ページ番号）を取得
                let entry_ptr = self.table_ptr.start.as_ptr().add(i as usize);
                let start_page = entry_ptr.read();

                // そのページの実体にある「サイズ情報」を取得
                let actual_addr = (start_page as usize) << 12;
                // ここで死ぬ可能性があるため、読み取り前にアドレスをチェック
                if actual_addr == 0 {
                    log_info!("kernel", "dump", "  [{}] NULL ADDRESS DETECTED!", i);
                    continue;
                }
                let size = (actual_addr as *const u32).read();

                log_info!(
                    "kernel",
                    "dump",
                    "  [{}] PageIdx: {:#X} (Addr: {:#X}) -> Size: {} pages",
                    i,
                    start_page,
                    actual_addr,
                    size
                );
            }
        }
    }

    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        // 要求サイズをページ単位(4096)に切り上げ
        let size_bytes = (layout.size() + 4095) & !4095;
        let pages_needed = (size_bytes >> 12) as u32;

        for i in 0..self.table_len {
            let entry_ptr = self.table_ptr.start.as_ptr().add(i as usize);
            let start_page = entry_ptr.read();
            let start_addr = (start_page as usize) << 12;

            // 空きブロックの先頭4バイトから現在のページ数を取得
            let available_pages = (start_addr as *const u32).read();

            if available_pages >= pages_needed {
                // 切り出した後の「新しい開始位置」
                let new_start_page = start_page + pages_needed;
                let new_available_pages = available_pages - pages_needed;

                #[cfg(all(feature = "enable_memory_poisoning", feature = "enable_overprotective_safety_checks"))]
                {
                    let check_ptr = (start_addr + 4) as *const u8;
                    let check_len = if layout.size() > 4 { layout.size() - 4 } else { 0 };

                    for offset in 0..check_len {
                        if unsafe{check_ptr.add(offset).read()} != POSITION_VALUE {
                            log_warn!("kernel", "alloc", "corruption at {:#x}", start_addr + 4 + offset);
                            break;
                        }
                    }
                }

                if new_available_pages == 0 {
                    remove(
                        self.table_ptr.start.as_ptr(),
                        i as usize,
                        self.table_len as usize,
                    );
                    self.table_len -= 1;
                } else {
                    entry_ptr.write(new_start_page);
                    let new_start_addr = (new_start_page as usize) << 12;
                    (new_start_addr as *mut u32).write(new_available_pages);
                }

                let _ = self.try_add_table_map();

                return start_addr as *mut u8;
            }
        }
        null_mut()
    }

    pub(crate) unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        let start_addr = (ptr as usize) & !4095;
        let end_addr = (ptr as usize + size + 4095) & !4095;

        #[cfg(feature = "enable_memory_poisoning")]
        {
            let range_size = end_addr - start_addr;
            unsafe {
                core::ptr::write_bytes(start_addr as *mut u8, POSITION_VALUE, range_size);
            }
        }

        let page_idx = (start_addr >> 12) as u32;
        let mut page_count = ((end_addr - start_addr) >> 12) as u32;

        let mut i = 0;
        while i < self.table_len {
            let existing_loc = unsafe { self.table_ptr.start.as_ptr().add(i as usize).read() };
            if existing_loc == page_idx + page_count {
                let existing_size =
                    unsafe { (((existing_loc as usize) << 12) as *const u32).read() };
                page_count += existing_size;
                remove(
                    self.table_ptr.start.as_ptr(),
                    i as usize,
                    self.table_len as usize,
                );
                self.table_len -= 1;
                continue;
            }
            i += 1;
        }

        // 上側との合体: page_idx == (loc + len)
        for i in 0..self.table_len {
            let entry_ptr = unsafe { self.table_ptr.start.as_ptr().add(i as usize) };
            let loc = unsafe { entry_ptr.read() as usize };
            let len = unsafe { ((loc << 12) as *const u32).read() } as usize;
            if page_idx == (loc + len) as u32 {
                let new_len = len + page_count as usize;
                unsafe { ((loc << 12) as *mut u32).write(new_len as u32) };
                return;
            }
        }

        // 挿入位置を探して追加
        let mut insert_idx = 0;
        while insert_idx < self.table_len {
            let val = unsafe {
                self.table_ptr
                    .start
                    .as_ptr()
                    .add(insert_idx as usize)
                    .read()
            };
            if val > page_idx {
                break;
            }
            insert_idx += 1;
        }

        unsafe { (((page_idx as usize) << 12) as *mut u32).write(page_count) };
        insert(
            self.table_ptr.start.as_ptr(),
            insert_idx as usize,
            page_idx,
            self.table_len as usize,
        );
        self.table_len += 1;
    }

    pub(crate) unsafe fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let ptr = unsafe { self.alloc(layout) };

        if ptr.is_null() {
            return null_mut();
        }

        let start = (ptr as usize) & !4095;
        let end = (ptr as usize + size + 4095) & !4095;
        let zero_len = end - start;

        unsafe {
            core::ptr::write_bytes(start as *mut u8, 0, zero_len);
        }

        ptr
    }

    pub(crate) unsafe fn realloc(
        &mut self,
        ptr: *mut u8,
        layout: Layout,
        new_size: usize,
    ) -> *mut u8 {
        let old_size = layout.size();
        let old_end = (ptr as usize + old_size + 4095) & !4095;

        let new_end = (ptr as usize + new_size + 4095) & !4095;

        if old_end == new_end {
            return ptr;
        }

        if new_size < old_size {
            let old_end = (ptr as usize + old_size + 4095) & !4095;
            let new_end = (ptr as usize + new_size + 4095) & !4095;

            if new_end < old_end {
                let surplus_ptr = new_end as *mut u8;
                let surplus_size = old_end - new_end;

                let surplus_layout =
                    unsafe { Layout::from_size_align_unchecked(surplus_size, 4096) };
                unsafe { self.dealloc(surplus_ptr, surplus_layout) };
            }
            return ptr;
        }

        let old_end_page = (old_end >> 12) as u32;
        let new_end_page = (new_end >> 12) as u32;
        let need_pages = (new_end_page - old_end_page) as usize;

        for i in 0..self.table_len {
            let entry_ptr = unsafe { self.table_ptr.start.as_ptr().add(i as usize) };
            let existing_loc = unsafe { entry_ptr.read() };

            if existing_loc == old_end_page {
                let existing_addr = (existing_loc as usize) << 12;
                let existing_size = unsafe { (existing_addr as *const u32).read() } as usize;

                if existing_size >= need_pages {
                    #[cfg(all(feature = "enable_memory_poisoning", feature = "enable_overprotective_safety_checks"))]
                    {
                        let check_ptr = (existing_loc + 4) as *const u8;
                        let check_len = if existing_size > 4 { existing_size - 4 } else { 0 };

                        for offset in 0..check_len {
                            if unsafe{check_ptr.add(offset).read()} != POSITION_VALUE {
                                log_warn!("kernel", "alloc", "corruption at {:#x}", (existing_loc + 4 + offset as u32));
                                break;
                            }
                        }
                    }

                    unsafe {
                        if existing_size == need_pages {
                            remove(
                                self.table_ptr.start.as_ptr(),
                                i as usize,
                                self.table_len as usize,
                            );
                            self.table_len -= 1;
                        } else {
                            let new_loc = existing_loc + need_pages as u32;
                            let new_size = existing_size - need_pages;
                            entry_ptr.write(new_loc);
                            (((new_loc as usize) << 12) as *mut u32).write(new_size as u32);
                        }
                    }
                    return ptr;
                }
                break;
            }
        }

        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        let new_ptr = unsafe { self.alloc(new_layout) };
        if !new_ptr.is_null() {
            unsafe {
                core::ptr::copy_nonoverlapping(ptr, new_ptr, old_size);
                self.dealloc(ptr, layout);
            }
        }
        new_ptr
    }
}

pub struct BoundaryTagFrameAllocator(pub(crate) spin::Mutex<InternalBoundaryTagFrameAllocator>);

impl BoundaryTagFrameAllocator {
    pub fn new(arg_target: MemData<usize>) -> Result<(MemData<usize>, Self), Error> {
        let (data, internal) = InternalBoundaryTagFrameAllocator::new(arg_target)?;
        Ok((data, Self(spin::Mutex::new(internal))))
    }
}

unsafe impl GlobalAlloc for BoundaryTagFrameAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { self.0.lock().alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.0.lock().dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { self.0.lock().alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe { self.0.lock().realloc(ptr, layout, new_size) }
    }
}

unsafe impl Send for InternalBoundaryTagFrameAllocator {}
unsafe impl Sync for InternalBoundaryTagFrameAllocator {}
