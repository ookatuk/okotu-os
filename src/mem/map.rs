use crate::mem::types::MemMap;
use alloc::vec;
use alloc::vec::Vec;
use uefi::mem::memory_map::{MemoryMap, MemoryMapOwned};
use uefi_raw::table::boot::MemoryType;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MemoryMapType {
    KernelData,
    KernelCode,
    UefiRuntimeServiceAllocated,
    UefiRuntimeServiceCode,
    UefiBootServicesAllocated,
    NotAllocatedByUefiAllocator,
    Broken,
    Used,
    Acpi,
    AcpiTable,
    NonVolatile,
    UsedByHardWare,

    Other,
    Mmio,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Map {
    pub data: MemMap,
    pub memory_type: MemoryMapType,
}

impl Map {
    pub const fn new(start: usize, end: usize, memory_type: MemoryMapType) -> Map {
        Map {
            data: MemMap {
                start: start as u64,
                end: end as u64,
            },
            memory_type,
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct MemMapping(pub(crate) Vec<Map>);

impl MemMapping {
    pub fn sort(&mut self) {
        self.0.sort_unstable_by_key(|m| m.data.start)
    }

    pub fn check(&self) -> bool {
        let mut last_end = 0;

        for i in &self.0 {
            if i.data.start < last_end || i.data.start > i.data.end {
                return false;
            }
            last_end = i.data.end;
        }
        true
    }

    pub fn clean(&mut self) {
        self.0.retain(|m| m.data.start < m.data.end);
    }

    pub fn minimize(&mut self) {
        if self.0.is_empty() {
            return;
        }

        self.clean();

        let mut w = 0;

        for r in 1..self.0.len() {
            if self.0[w].memory_type == self.0[r].memory_type
                && self.0[w].data.end == self.0[r].data.start
            {
                self.0[w].data.end = self.0[r].data.end;
            } else {
                w += 1;
                if w != r {
                    self.0[w] = self.0[r].clone();
                }
            }
        }

        self.0.truncate(w + 1);
        self.0.shrink_to_fit();
    }

    pub fn add_me_to_memory_map(&mut self) {
        self.0.reserve(10);

        let data_ptr = self.0.as_ptr() as usize;
        let total_size = self.0.capacity() * size_of::<Map>();

        self.change(
            MemoryMapType::KernelData,
            MemMap {
                start: data_ptr as u64,
                end: (data_ptr + total_size) as u64,
            },
            false,
        );
    }

    pub fn think_add(&mut self, data: Map, allow_move_and_auto_minimize: bool) {
        let prev_idx = self
            .0
            .iter()
            .position(|m| m.data.end == data.data.start && m.memory_type == data.memory_type);
        let next_idx = self
            .0
            .iter()
            .position(|m| m.data.start == data.data.end && m.memory_type == data.memory_type);

        match (prev_idx, next_idx) {
            (Some(p), Some(n)) => {
                self.0[p].data.end = self.0[n].data.end;
                self.0.remove(n);
            }
            (Some(p), None) => {
                self.0[p].data.end = data.data.end;
            }
            (None, Some(n)) => {
                self.0[n].data.start = data.data.start;
            }
            (None, None) => {
                self.0.push(data);
            }
        }

        if allow_move_and_auto_minimize {
            self.0.shrink_to_fit();
        }
    }

    pub fn remove_range(&mut self, data: MemMap, allow_move_and_auto_minimize: bool) {
        let start = data.start;
        let end = data.end;

        let mut i = 0;
        while i < self.0.len() {
            let entry_start = self.0[i].data.start;
            let entry_end = self.0[i].data.end;

            if entry_end <= start || entry_start >= end {
                i += 1;
                continue;
            }

            if entry_start >= start && entry_end <= end {
                self.0.remove(i);
            } else if entry_start < start && entry_end > end {
                let old_end = entry_end;
                self.0[i].data.end = start;
                let mut new_entry = self.0[i].clone();
                new_entry.data.start = end;
                new_entry.data.end = old_end;
                self.0.insert(i + 1, new_entry);
                i += 2;
            } else if entry_start < start {
                self.0[i].data.end = start;
                i += 1;
            } else {
                self.0[i].data.start = end;
                i += 1;
            }
        }
        if allow_move_and_auto_minimize {
            self.0.shrink_to_fit();
        }
    }

    pub fn change(
        &mut self,
        m_type: MemoryMapType,
        data: MemMap,
        allow_move_andauto_minimize: bool,
    ) {
        self.remove_range(data.clone(), false);
        self.think_add(
            Map {
                data,
                memory_type: m_type,
            },
            false,
        );

        if allow_move_andauto_minimize {
            self.0.shrink_to_fit();
        }
    }
}

impl From<&MemoryMapOwned> for MemMapping {
    fn from(memory_map: &MemoryMapOwned) -> Self {
        let mut data = vec![];
        data.reserve(memory_map.len());
        for i in memory_map.entries() {
            let mtype = match i.ty {
                MemoryType::RESERVED => MemoryMapType::Used,

                MemoryType::LOADER_DATA => MemoryMapType::KernelData,
                MemoryType::LOADER_CODE => MemoryMapType::KernelCode,

                MemoryType::ACPI_NON_VOLATILE => MemoryMapType::Acpi,
                MemoryType::ACPI_RECLAIM => MemoryMapType::AcpiTable,

                MemoryType::BOOT_SERVICES_CODE => MemoryMapType::UefiBootServicesAllocated,
                MemoryType::BOOT_SERVICES_DATA => MemoryMapType::UefiBootServicesAllocated,

                MemoryType::PAL_CODE => MemoryMapType::UsedByHardWare,
                MemoryType::RUNTIME_SERVICES_CODE => MemoryMapType::UefiRuntimeServiceCode,
                MemoryType::RUNTIME_SERVICES_DATA => MemoryMapType::UefiRuntimeServiceAllocated,

                MemoryType::PERSISTENT_MEMORY => MemoryMapType::NonVolatile,

                MemoryType::MMIO_PORT_SPACE => MemoryMapType::Mmio,
                MemoryType::MMIO => MemoryMapType::Mmio,

                MemoryType::CONVENTIONAL => MemoryMapType::NotAllocatedByUefiAllocator,

                _ => MemoryMapType::Other,
            };

            data.push(Map::new(
                i.phys_start as usize,
                (i.phys_start + (i.page_count * 4096)) as usize,
                mtype,
            ));
        }

        Self(data)
    }
}
