use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::ptr::NonNull;
use core::time::Duration;
use acpi::{Handle, PciAddress, PhysicalMapping};
use acpi::aml::AmlError;
use spin::{Lazy, Mutex, Once};
use x86_64::instructions::interrupts::without_interrupts;
use x86_64::instructions::port::Port;
use x86_64::VirtAddr;
use crate::{deb, MAIN_COPY};
use crate::memory::paging::{get_addr, PageEntryFlags, PHY_OFFSET};
use crate::timer::Timer;
use crate::timer::tsc::{Tsc, TSC};
use crate::util_types::MemRangeData;

struct AmlMutexInner {
    lock: Mutex<()>,
    owner: Mutex<Option<u32>>,
    count: Mutex<u32>,
}

pub static AML_MUTEXES: Lazy<Arc<Mutex<Vec<AmlMutexInner>>>> = Lazy::new(|| {Arc::new(Mutex::new(Vec::new()))});

#[derive(Clone, Copy)]
pub struct TmpHandler {
}

impl TmpHandler {
    pub fn init(&self){

    }

    pub fn new() -> Self {
        Self {

        }
    }
}

impl acpi::Handler for TmpHandler {
    unsafe fn map_physical_region<T>(&self, physical_address: usize, size: usize) -> PhysicalMapping<Self, T> {
        let mcp = MAIN_COPY.get().unwrap();

        let page_base = physical_address & !0xFFF;
        let offset = physical_address - page_base;
        let aligned_size = (size + offset + 0xFFF) & !0xFFF;

        let a = || {
            mcp.util_update_add_paging::<true>(
                vec![MemRangeData::new(
                    page_base,
                    aligned_size
                )],
                vec![
                    PageEntryFlags::PRESENT |
                        PageEntryFlags::WRITABLE |
                        PageEntryFlags::PCD |
                        PageEntryFlags::EXECUTE_DISABLE,
                ],
            ).unwrap();
        };

        if get_addr(VirtAddr::new(physical_address as u64)).is_err() {
            a();
        }

        PhysicalMapping {
            physical_start: physical_address,
            virtual_start: NonNull::new(physical_address as *mut T).expect("Virtual address is null"),
            region_length: size,
            mapped_length: aligned_size,
            handler: self.clone(),
        }
    }


    fn unmap_physical_region<T>(region: &PhysicalMapping<Self, T>) {
        return
    }

    fn read_u8(&self, address: usize) -> u8 {
        unsafe { (address as *const u8).read_volatile() }
    }

    fn read_u16(&self, address: usize) -> u16 {
        unsafe { (address as *const u16).read_volatile() }
    }

    fn read_u32(&self, address: usize) -> u32 {
        unsafe { (address as *const u32).read_volatile() }
    }

    fn read_u64(&self, address: usize) -> u64 {
        unsafe { (address as *const u64).read_volatile() }
    }

    fn write_u8(&self, address: usize, value: u8) {
        unsafe { (address as *mut u8).write_volatile(value) }
    }

    fn write_u16(&self, address: usize, value: u16) {
        unsafe { (address as *mut u16).write_volatile(value) }
    }

    fn write_u32(&self, address: usize, value: u32) {
        unsafe { (address as *mut u32).write_volatile(value) }
    }

    fn write_u64(&self, address: usize, value: u64) {
        unsafe { (address as *mut u64).write_volatile(value) }
    }

    fn read_io_u8(&self, port: u16) -> u8 {
        unsafe { Port::new(port).read() }
    }

    fn read_io_u16(&self, port: u16) -> u16 {
        unsafe { Port::new(port).read() }
    }

    fn read_io_u32(&self, port: u16) -> u32 {
        unsafe { Port::new(port).read() }
    }

    fn write_io_u8(&self, port: u16, value: u8) {
        unsafe { Port::new(port).write(value) }
    }

    fn write_io_u16(&self, port: u16, value: u16) {
        unsafe { Port::new(port).write(value) }
    }

    fn write_io_u32(&self, port: u16, value: u32) {
        unsafe { Port::new(port).write(value) }
    }

    fn read_pci_u8(&self, address: PciAddress, offset: u16) -> u8 {
        let addr = 0x8000_0000u32
            | ((address.bus() as u32) << 16)
            | ((address.device() as u32) << 11)
            | ((address.function() as u32) << 8)
            | ((offset & 0xfc) as u32);

        unsafe {
            Port::<u32>::new(0xCF8).write(addr);

            let val: u32 = Port::<u32>::new(0xCFC).read();

            (val >> ((offset & 0x3) * 8)) as u8
        }
    }

    fn read_pci_u16(&self, address: PciAddress, offset: u16) -> u16 {
        let addr = 0x8000_0000u32
            | ((address.bus() as u32) << 16)
            | ((address.device() as u32) << 11)
            | ((address.function() as u32) << 8)
            | ((offset & 0xfc) as u32);

        unsafe {
            Port::<u32>::new(0xCF8).write(addr);
            let val = Port::<u32>::new(0xCFC).read();

            (val >> ((offset & 0x2) * 8)) as u16
        }
    }

    fn read_pci_u32(&self, address: PciAddress, offset: u16) -> u32 {
        let addr = 0x8000_0000u32
            | ((address.bus() as u32) << 16)
            | ((address.device() as u32) << 11)
            | ((address.function() as u32) << 8)
            | ((offset & 0xfc) as u32);

        unsafe {
            Port::<u32>::new(0xCF8).write(addr);
            Port::<u32>::new(0xCFC).read()
        }
    }

    fn write_pci_u8(&self, address: PciAddress, offset: u16, value: u8) {
        let old_val = self.read_pci_u32(address, offset);

        let shift = (offset & 0x3) * 8;

        let new_val = (old_val & !(0xff << shift)) | ((value as u32) << shift);

        self.write_pci_u32(address, offset, new_val);
    }

    fn write_pci_u16(&self, address: PciAddress, offset: u16, value: u16) {
        let old_val = self.read_pci_u32(address, offset);
        let shift = (offset & 0x2) * 8;
        let new_val = (old_val & !(0xffff << shift)) | ((value as u32) << shift);
        self.write_pci_u32(address, offset, new_val);
    }

    fn write_pci_u32(&self, address: PciAddress, offset: u16, value: u32) {
        let addr = 0x8000_0000u32
            | ((address.bus() as u32) << 16)
            | ((address.device() as u32) << 11)
            | ((address.function() as u32) << 8)
            | ((offset & 0xfc) as u32);

        unsafe {
            Port::<u32>::new(0xCF8).write(addr);
            Port::<u32>::new(0xCFC).write(value);
        }
    }

    fn nanos_since_boot(&self) -> u64 {
        TSC.get_time().as_nanos() as u64
    }

    fn stall(&self, microseconds: u64) {
        TSC.spin(Duration::from_micros(microseconds))
    }

    fn sleep(&self, milliseconds: u64) {
        self.stall(milliseconds * 1000);
    }

    fn create_mutex(&self) -> Handle {
        let mut mutexes = AML_MUTEXES.lock();
        let handle = Handle(mutexes.len() as u32);

        mutexes.push(AmlMutexInner {
            lock: Mutex::new(()),
            owner: Mutex::new(None),
            count: Mutex::new(0),
        });

        handle
    }

    fn acquire(&self, handle: Handle, timeout: u16) -> Result<(), AmlError> {
        let mutexes = AML_MUTEXES.lock();
        let aml_mutex = &mutexes[handle.0 as usize];

        let my_id = match crate::cpu::utils::who_am_i() {
            Some(id) => id,
            None => {
                return Err(AmlError::PrtInvalidAddress)
            }
        };

        if *aml_mutex.owner.lock() == Some(my_id) {
            *aml_mutex.count.lock() += 1;
            return Ok(());
        }

        let mut elapsed = 0;
        loop {
            if let Some(_guard) = aml_mutex.lock.try_lock() {
                core::mem::forget(_guard);

                *aml_mutex.owner.lock() = Some(my_id);
                *aml_mutex.count.lock() = 1;
                return Ok(());
            }

            if timeout != 0xFFFF && elapsed >= timeout {
                return Err(AmlError::MutexAcquireTimeout);
            }

            self.stall(1000);
            elapsed += 1;
        }
    }

    fn release(&self, handle: Handle) {
        let mutexes = AML_MUTEXES.lock();
        let aml_mutex = &mutexes[handle.0 as usize];

        let mut count = aml_mutex.count.lock();
        if *count > 0 {
            *count -= 1;

            if *count == 0 {
                *aml_mutex.owner.lock() = None;
                unsafe { aml_mutex.lock.force_unlock(); }
            }
        }
    }
}