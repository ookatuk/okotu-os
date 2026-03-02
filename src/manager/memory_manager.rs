use crate::mem::allocator::main::HybridAllocator;
use crate::mem::map::{MemMapping, MemoryMapType};
use crate::mem::types::{MemData, MemMap};
use crate::util::result;
use crate::util::result::{Error, ErrorType};
use crate::{log_debug, log_info};
use alloc::sync::Arc;
use alloc::vec;
use core::alloc::Layout;
use core::ptr::addr_of;
use spin::{Once, RwLock};
use uefi::mem::memory_map::{MemoryMap, MemoryMapOwned};
use uefi_raw::table::boot::MemoryType;
use x86_64::instructions::interrupts;
use x86_64::instructions::interrupts::without_interrupts;

const FIRST_ALLOC: usize = 1024 * 1024 * 50;

#[derive(Default)]
pub struct MemoryManager {
    pub do_fn: Once<Arc<dyn Fn()>>,
    pub uefi_memory_map: Once<Arc<RwLock<MemMapping>>>,
    pub internal_init_mem_tmp_alloc: RwLock<Option<HybridAllocator>>,
}

impl MemoryManager {
    pub fn create_memory_map(&self) -> result::Result {
        log_info!("kernel", "memory", "getting mapping...");
        let memory_map = Error::try_raise(
            uefi::boot::memory_map(MemoryType::LOADER_DATA),
            Some("failed to get memory map"),
        )?;

        let len = memory_map.len();

        let mut map = Arc::new(RwLock::new(MemMapping::from(&memory_map)));

        interrupts::without_interrupts(|| {
            let mut map = Arc::get_mut(&mut map).unwrap().write();

            map.sort();

            if !map.check() {
                return Error::new(ErrorType::UefiBroken, Some("invalid memory map.")).raise();
            }

            self.do_fn.get().unwrap()();

            log_info!("kernel", "memory", "optimizing mapping...");

            let uefi_map_ptr = memory_map
                .entries()
                .next()
                .map(|desc| desc as *const _ as usize)
                .unwrap_or(0);

            let desc_size = memory_map.meta().desc_size;

            let uefi_map_size = memory_map.len() * desc_size;

            map.change(
                MemoryMapType::NotAllocatedByUefiAllocator,
                MemMap {
                    start: uefi_map_ptr as u64,
                    end: (uefi_map_ptr + uefi_map_size) as u64,
                },
                false,
            );

            let struct_ptr = addr_of!(memory_map).addr();
            let struct_size = size_of::<MemoryMapOwned>();

            map.change(
                MemoryMapType::NotAllocatedByUefiAllocator,
                MemMap {
                    start: struct_ptr as u64,
                    end: (struct_ptr + struct_size) as u64,
                },
                false,
            );

            self.do_fn.get().unwrap()();

            log_debug!("kernel", "memory", "removed old mapping.");

            map.minimize();
            map.add_me_to_memory_map();
            map.sort();

            Ok(())
        })?;

        let new_len = map.read().0.len();

        let map2 = map.clone();

        if self.uefi_memory_map.is_completed() {
            let mut a = self.uefi_memory_map.get().unwrap().write();
            *a = map.read().clone();
        } else {
            self.uefi_memory_map.call_once(|| map2);
        }

        log_debug!("kernel", "memory", "optimized {} to {}", len, new_len);
        log_info!("kernel", "memory", "mapping ready. ({})", new_len);

        self.do_fn.get().unwrap()();

        Ok(())
    }

    pub fn create_tmp_allocator(&self, size: usize) -> result::Result {
        log_info!(
            "kernel",
            "memory",
            "allocating new allocator management memory"
        );
        let layout = Layout::from_size_align(size, 4096).unwrap();

        let allocated = unsafe { alloc::alloc::alloc_zeroed(layout) };

        self.do_fn.get().unwrap()();
        log_info!("kernel", "kernel", "creating new allocator...");

        let map = MemData {
            start: allocated.addr(),
            len: size,
        };

        self.create_memory_map()?;
        let len = self.uefi_memory_map.get().unwrap().read().0.len();

        without_interrupts(|| {
            let mut lock = self.internal_init_mem_tmp_alloc.write();
            *lock = Some(HybridAllocator::new(vec![map], len));
        });

        self.do_fn.get().unwrap()();
        log_info!("kernel", "kernel", "created new allocator.");

        Ok(())
    }

    pub fn add_allocators(&self) -> result::Result {
        let map = self.uefi_memory_map.get().unwrap().read();
        self.do_fn.get().unwrap()();

        let mut l: u64 = 0;
        let mut size: u64 = 0;

        for i in map.0.iter() {
            if i.memory_type != MemoryMapType::NotAllocatedByUefiAllocator {
                continue;
            }
            let si = (i.data.end - i.data.start);
            size += si;

            crate::ALLOC.os_allocator.get().unwrap().add(&vec![MemData {
                start: i.data.start as usize,
                len: si as usize,
            }]);
            l += 1;
        }

        log_info!(
            "kernel",
            "information",
            "added allocators ({} items. size: {}MiB)",
            l,
            size / 1024 / 1024
        );

        Ok(())
    }

    pub unsafe fn init_memory(&self, size: Option<usize>) -> result::Result {
        self.create_tmp_allocator(size.unwrap_or(FIRST_ALLOC))?;
        self.create_memory_map()?;

        log_info!(
            "kernel",
            "information",
            "From now on, until the full allocator is complete, logging will be low due to memory limitations. (the temp global allocator has only {}MB available)",
            size.unwrap_or(FIRST_ALLOC) / 1024 / 1024
        );

        let allocator = self.internal_init_mem_tmp_alloc.write().take().unwrap();

        crate::ALLOC.enable_os_allocator(allocator);

        self.do_fn.get().unwrap()();

        log_info!("kernel", "information", "changed allocator.");

        Ok(())
    }

    pub unsafe fn add_alloc(&self) -> result::Result {
        log_info!("kernel", "allocator", "creating full allocators...");

        self.add_allocators()?;

        self.do_fn.get().unwrap()();

        Ok(())
    }
}
