use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use acpi::{aml, Handler, PhysicalMapping};
use acpi::sdt::facs::Facs;
use spin::Mutex;
use crate::acpi::core::ACPI_TABLE_TMP_HANDLER;
use crate::acpi::handler::TmpHandler;

pub fn spawn_on() {

    let tmphand = crate::acpi::handler::TmpHandler::new();

    let tab = ACPI_TABLE_TMP_HANDLER.get().unwrap();
    for i in tab.find_tables::<acpi::sdt::fadt::Fadt>() {
        let revision = i.header.revision;

        let dsdt_addr = i.dsdt_address().unwrap();
        let regs = Arc::new(acpi::registers::FixedRegisters::new(
            i.get().get_ref(),
            tmphand
        ).unwrap());

        let x_firmware_ctrl = i.x_firmware_ctrl;
        let firmware_ctrl = i.firmware_ctrl;

        let facs_phys_addr = if revision >= 2 {
            if let Some(addr) = unsafe { x_firmware_ctrl.access(revision) } {
                if addr != 0 { addr as usize } else { firmware_ctrl as usize }
            } else {
                firmware_ctrl as usize
            }
        } else {
            firmware_ctrl as usize
        };

        let facs_mapping: Option<PhysicalMapping<TmpHandler, Facs>> = if facs_phys_addr != 0 {
            Some(unsafe {
                tmphand.map_physical_region::<Facs>(
                    facs_phys_addr as usize,
                    size_of::<Facs>()
                )
            })
        } else {
            None
        };

        let mut a = acpi::aml::Interpreter::new(
            tmphand,
            revision,
            regs,
            facs_mapping,
        );
    }
}
