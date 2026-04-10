use alloc::alloc::{alloc};
use core::alloc::Layout;
use core::ptr;
use uefi::boot;
use uefi::table::system_table_raw;
use uefi_raw::Status;
use uefi_raw::table::boot::{MemoryDescriptor};

pub struct MemoryMapIter {
    ptr: *const u8,
    end: *const u8,
    desc_size: usize,
}

impl Iterator for MemoryMapIter {
    type Item = &'static MemoryDescriptor;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr >= self.end {
            return None;
        }
        let entry = unsafe { &*(self.ptr as *const MemoryDescriptor) };
        self.ptr = unsafe { self.ptr.add(self.desc_size) };
        Some(entry)
    }
}

#[derive(Debug)]
pub struct MyMemoryMapOwned {
    ptr: *mut u8,
    #[allow(unused)]
    layout: Layout,
    total_size: usize,
    desc_size: usize,
}

impl MyMemoryMapOwned {
    pub fn iter(&self) -> MemoryMapIter {
        MemoryMapIter {
            ptr: self.ptr,
            end: unsafe { self.ptr.add(self.total_size) },
            desc_size: self.desc_size,
        }
    }
}

pub unsafe fn exit_boot_services_with_talc() -> MyMemoryMapOwned {
    let bt_ptr = unsafe { system_table_raw().unwrap().read().boot_services };
    let bt = unsafe { bt_ptr.as_ref() }.expect("Failed to get BS");

    let mut map_size = 0;
    let mut map_key = 0;
    let mut desc_size = 0;
    let mut desc_version = 0;

    unsafe {
        let _ = (bt.get_memory_map)(
            &mut map_size,
            ptr::null_mut(),
            &mut map_key,
            &mut desc_size,
            &mut desc_version,
        );
    }

    assert!(desc_size > 0, "desc_size is 0");

    let extra_space = desc_size * 8;
    let buffer_capacity = map_size + extra_space;
    let layout = Layout::from_size_align(buffer_capacity, 4096).unwrap();
    let buffer_ptr = unsafe { alloc(layout) };

    if buffer_ptr.is_null() {
        panic!("Out of memory for memory map");
    }

    let mut map = MyMemoryMapOwned {
        ptr: buffer_ptr,
        layout,
        total_size: buffer_capacity,
        desc_size,
    };

    for _ in 0..3 {
        let mut actual_map_size = buffer_capacity;
        let status = unsafe {
            (bt.get_memory_map)(
                &mut actual_map_size,
                buffer_ptr as *mut _,
                &mut map_key,
                &mut desc_size,
                &mut desc_version,
            )
        };

        if status == Status::SUCCESS {
            let image_handle = boot::image_handle();
            let exit_status = unsafe { (bt.exit_boot_services)(image_handle.as_ptr(), map_key) };

            if exit_status == Status::SUCCESS {
                map.total_size = actual_map_size;
                return map;
            }
        }
    }

    panic!("Failed to exit boot services");
}