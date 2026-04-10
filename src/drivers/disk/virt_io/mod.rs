use alloc::vec::Vec;
use acpi::{AcpiTable, Handler};
use crate::{log_info, log_warn};
use crate::asy_nc::yield_now;

pub async fn a() {
    let table = crate::acpi::core::ACPI_TABLE_TMP_HANDLER.get().unwrap();

    let scan_targets: Vec<_> = table
        .find_tables::<acpi::sdt::mcfg::Mcfg>()
        .filter_map(|mcfg| {
            if mcfg.validate().is_ok() {
                let h = mcfg.handler.clone();
                Some(mcfg.entries().iter().map(|&e| (e, h.clone())).collect::<Vec<_>>())
            } else {
                None
            }
        })
        .flatten()
        .collect();

    for (entry, handler) in scan_targets {
        let base_addr = entry.base_address as usize;

        for bus in entry.bus_number_start..=entry.bus_number_end {
            for dev in 0..32 {
                yield_now().await;
                for func in 0..8 {
                    let bus_offset = (bus - entry.bus_number_start) as usize;
                    let device_phys_addr = base_addr + (bus_offset << 20 | (dev as usize) << 15 | (func as usize) << 12);

                    {
                        let mapping = unsafe { handler.map_physical_region::<u32>(device_phys_addr, 4096) };
                        let ptr = mapping.virtual_start.as_ptr();

                        let id_reg = unsafe { core::ptr::read_volatile(ptr) };
                        let vendor_id = (id_reg & 0xFFFF) as u16;
                        let device_id = (id_reg >> 16) as u16;

                        if vendor_id == 0xFFFF {
                            continue;
                        }

                        if vendor_id == 0x1AF4 {
                            let mut cap_ptr = unsafe { (core::ptr::read_volatile(ptr.add(13)) & 0xFF) as u8 };
                            while cap_ptr != 0 {
                                let cap_base = (ptr as usize + cap_ptr as usize) as *const u8;
                                let cap_id = unsafe { core::ptr::read_volatile(cap_base) };

                                if cap_id == 0x09 {
                                    let cfg_type = unsafe { core::ptr::read_volatile(cap_base.add(3)) };
                                    let bar_index = unsafe { core::ptr::read_volatile(cap_base.add(4)) };
                                    let offset = unsafe { core::ptr::read_volatile(cap_base.add(8) as *const u32) };

                                    log_info!("kernel", "virt-io", "Modern Cap Type: {}, BAR: {}, Offset: {:#x}, Device: {}", cfg_type, bar_index, offset, device_id);
                                }
                                cap_ptr = unsafe { core::ptr::read_volatile(cap_base.add(1)) };
                            }
                        }

                        if func == 0 {
                            let header_reg = unsafe { core::ptr::read_volatile(ptr.add(3)) };
                            let header_type = (header_reg >> 16) & 0xFF;
                            if (header_type & 0x80) == 0 {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}