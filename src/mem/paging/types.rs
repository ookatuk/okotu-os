use alloc::boxed::Box;
use bitflags::{bitflags, Flags};
use x86_64::structures::paging;
use x86_64::structures::paging::PageTableFlags;
use x86_64::{PhysAddr};
use crate::mem::types::{MemData};
use crate::util::result;
use crate::util::result::{ErrorType};
use itertools::izip;

bitflags! {
    pub struct PageEntryFlags: u64 {
        const PRESENT = 1 << 0;
        const WRITABLE = 1 << 1;
        const USER_PAGE = 1 << 2;
        const DISABLE_WRITE_CHACHE = 1 << 3;
        const DISABLE_CHACHE = 1 << 4;
        const ACCESSED = 1 << 5;
        const DIRTY = 1 << 6;
        const PAT = 1 << 7;
        const GLOBAL = 1 << 8;
        const EXECUTE_DISABLE = 1 << 63;
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PageEntryType {
    Pt,
    Huge,
    Normal,
}

impl PageEntryFlags {
    pub fn to_x86_64_page_table_flags(&self, mode: PageEntryType) -> PageTableFlags {
        let mut flags = PageTableFlags::empty();

        if self.contains(Self::PRESENT) { flags |= PageTableFlags::PRESENT; }
        if self.contains(Self::WRITABLE) { flags |= PageTableFlags::WRITABLE; }
        if self.contains(Self::USER_PAGE) { flags |= PageTableFlags::USER_ACCESSIBLE; } // 追加
        if self.contains(Self::DISABLE_WRITE_CHACHE) { flags |= PageTableFlags::WRITE_THROUGH; }
        if self.contains(Self::DISABLE_CHACHE) { flags |= PageTableFlags::NO_CACHE; }
        if self.contains(Self::ACCESSED) { flags |= PageTableFlags::ACCESSED; }
        if self.contains(Self::DIRTY) { flags |= PageTableFlags::DIRTY; }
        if self.contains(Self::GLOBAL) { flags |= PageTableFlags::GLOBAL; }
        if self.contains(Self::EXECUTE_DISABLE) { flags |= PageTableFlags::NO_EXECUTE; }

        match mode {
            PageEntryType::Pt => {
                if self.contains(Self::PAT) {
                    flags |= PageTableFlags::HUGE_PAGE;
                }
            }
            PageEntryType::Huge => {
                flags |= PageTableFlags::HUGE_PAGE;

                if self.contains(Self::PAT) {
                    flags |= PageTableFlags::from_bits_retain(1 << 12);
                }
            }
            PageEntryType::Normal => {
            }
        }

        flags
    }
}

fn create_pd(map_list: &[MemData], flags: &[PageEntryFlags], huges: &[bool]) -> result::Result<&'static mut paging::PageTable> {
    let mut table = Box::new(paging::PageTable::new());

    if map_list.len() != flags.len() || map_list.len() != huges.len() {
        return result::Error::new(
            ErrorType::InvalidData,
            Some("The number of mappings and flags does not match.")
        ).raise();
    }

    for (map, flag, entry, huge) in izip!(map_list.iter(), flags.iter(), table.iter_mut(), huges.iter()) {
        if *huge && (map.len != 1024 * 1024 * 2 || map.start % (1024 * 1024 * 2) != 0) {
            return result::Error::new(
                ErrorType::InvalidData,
                Some("The PD must be 2 Mib in length and align.")
            ).raise();
        }

        entry.set_addr(
            PhysAddr::new(map.start),
            flag.to_x86_64_page_table_flags(if *huge {PageEntryType::Huge} else {PageEntryType::Normal}),
        );
    }

    Ok(Box::leak(table))
}

fn create_pt(map_list: &[MemData], flags: &[PageEntryFlags]) -> result::Result<&'static mut paging::PageTable> {
    let mut table = Box::new(paging::PageTable::new());

    if map_list.len() != flags.len() {
        return result::Error::new(
            ErrorType::InvalidData,
            Some("The number of mappings and flags does not match.")
        ).raise();
    }

    for (map, flag, entry) in izip!(map_list.iter(), flags.iter(), table.iter_mut()) {
        if map.len != 4096 || map.start % 4096 != 0 {
            return result::Error::new(
                ErrorType::InvalidData,
                Some("The PT must be 4 kib in length and align.")
            ).raise();
        }

        entry.set_addr(
            PhysAddr::new(map.start),
            flag.to_x86_64_page_table_flags(PageEntryType::Pt),
        );
    }

    Ok(Box::leak(table))
}