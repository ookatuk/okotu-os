use alloc::vec::Vec;
use core::alloc::Layout;
use core::ptr::NonNull;
use acpi::{AcpiTable, Handler};
use crate::asy_nc::yield_now;
use crate::log_info;

const VIRT_IO_VENDOR_ID: u16 = 0x1AF4;
const VSC_ID: u8 = 0x9;
const CFG_PTR: usize = 0x3;
const BAR_IND_ID: usize = 0x4;
const BAR_OFF_PTR: usize = 0x8;
const BAR_LEN_PTR: usize = 0xC;
const CAP_PTR: usize = 0xD;
const BAR0_PTR: usize = 4;
const COMMAND_PTR: usize = 1;
const CAPABILITIES_PTR: usize = 13;

#[repr(C, packed)]
pub struct VirtioPciCommonCfg {
    pub device_feature_select: u32,
    pub device_feature: u32,
    pub driver_feature_select: u32,
    pub driver_feature: u32,
    pub config_msix_vector: u16,
    pub num_queues: u16,
    pub device_status: u8,
    pub config_generation: u8,
    pub queue_select: u16,
    pub queue_size: u16,
    pub queue_msix_vector: u16,
    pub queue_enable: u16,
    pub queue_notify_off: u16,
    pub queue_desc: u64,
    pub queue_driver: u64,
    pub queue_device: u64,
}

pub struct VirtIODevice {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub device_id: u16,
    pub common_cfg: Option<NonNull<VirtioPciCommonCfg>>,
    pub notify_base: Option<*mut u8>,
    pub device_cfg: Option<*mut u8>,
    pub isr_status: Option<*mut u8>,
}

unsafe impl Send for VirtIODevice {}
unsafe impl Sync for VirtIODevice {}

pub async fn a() -> Vec<VirtIODevice> {
    let mut found_devices = Vec::new();
    let table = crate::acpi::core::ACPI_TABLE_TMP_HANDLER.get().unwrap();

    let scan_targets: Vec<_> = table
        .find_tables::<acpi::sdt::mcfg::Mcfg>()
        .filter_map(|mcfg| {
            #[cfg(feature = "enable_required_safety_checks")]
            if mcfg.validate().is_ok() {
                let h = mcfg.handler.clone();
                Some(mcfg.entries().iter().map(|&e| (e, h.clone())).collect::<Vec<_>>())
            } else {
                None
            }
            #[cfg(not(feature = "enable_required_safety_checks"))]
            {
                let h = mcfg.handler.clone();
                Some(mcfg.entries().iter().map(|&e| (e, h.clone())).collect::<Vec<_>>())
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

                    let mapping = unsafe { handler.map_physical_region::<u32>(device_phys_addr, 4096) };
                    let pci_cfg_ptr = mapping.virtual_start.as_ptr();

                    let id_reg = unsafe { core::ptr::read_volatile(pci_cfg_ptr) };
                    let vendor_id = (id_reg & 0xFFFF) as u16;
                    let device_id = (id_reg >> 16) as u16;

                    let cmd = unsafe { core::ptr::read_volatile(pci_cfg_ptr.add(COMMAND_PTR)) };
                    unsafe { core::ptr::write_volatile(pci_cfg_ptr.add(1) as *mut u32, cmd | 0x02) };

                    if vendor_id == 0xFFFF { continue; }

                    if vendor_id == VIRT_IO_VENDOR_ID {
                        let mut virtio_dev = VirtIODevice {
                            bus, dev: dev as u8, func: func as u8,
                            device_id,
                            common_cfg: None, notify_base: None,
                            device_cfg: None, isr_status: None,
                        };

                        let mut cap_ptr = unsafe { (core::ptr::read_volatile(pci_cfg_ptr.add(CAP_PTR)) & 0xFF) as u8 };

                        while cap_ptr != 0 {
                            let cap_base = (pci_cfg_ptr as usize + cap_ptr as usize) as *const u8;
                            let cap_id = unsafe { core::ptr::read_volatile(cap_base) };

                            if cap_id == VSC_ID {
                                let cfg_type = unsafe { core::ptr::read_volatile(cap_base.add(CFG_PTR)) };
                                let bar_index = unsafe { core::ptr::read_volatile(cap_base.add(BAR_IND_ID)) };

                                let offset = unsafe { core::ptr::read_volatile(cap_base.add(BAR_OFF_PTR) as *const u32) };

                                let length = unsafe { core::ptr::read_volatile(cap_base.add(BAR_LEN_PTR) as *const u32) };

                                let mut bar_val = unsafe { core::ptr::read_volatile(pci_cfg_ptr.add(BAR0_PTR + bar_index as usize)) };

                                let bar_phys = (bar_val & !0xF) as usize;

                                if (bar_val & !0xF) == 0 {
                                    let layout = Layout::from_size_align(length as usize, length as usize).unwrap();

                                    let mmio_ptr = unsafe { alloc::alloc::alloc(layout) };

                                    if mmio_ptr.is_null() {
                                        panic!("Out of memory for PCI BAR allocation");
                                    }

                                    let assigned_phys = mmio_ptr as usize;

                                    unsafe {
                                        core::ptr::write_volatile(
                                            pci_cfg_ptr.add(BAR0_PTR + bar_index as usize) as *mut u32,
                                            assigned_phys as u32
                                        );
                                    }

                                    bar_val = assigned_phys as u32;
                                }

                                let reg_mapping = unsafe {
                                    handler.map_physical_region::<u8>(
                                        bar_phys + offset as usize,
                                        length as usize
                                    )
                                };

                                let reg_virt = reg_mapping.virtual_start.as_ptr();

                                match cfg_type {
                                    1 => virtio_dev.common_cfg = Some(NonNull::new(reg_virt as *mut VirtioPciCommonCfg).unwrap()),
                                    2 => virtio_dev.notify_base = Some(reg_virt),
                                    3 => virtio_dev.isr_status = Some(reg_virt),
                                    4 => virtio_dev.device_cfg = Some(reg_virt),
                                    _ => {}
                                }
                            }
                            cap_ptr = unsafe { core::ptr::read_volatile(cap_base.add(1)) };
                        }

                        log_info!("kernel", "virt-io", "Found VirtIO Device: ID={:#x} at {}:{}:{}", device_id, bus, dev, func);
                        found_devices.push(virtio_dev);
                    }

                    if func == 0 {
                        let header_type = (unsafe { core::ptr::read_volatile(pci_cfg_ptr.add(3)) } >> 16) & 0xFF;
                        if (header_type & 0x80) == 0 { break; }
                    }
                }
            }
        }
    }
    found_devices
}