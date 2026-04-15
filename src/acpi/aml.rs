use alloc::sync::Arc;
use core::hint::{cold_path, likely, unlikely};
use core::ops::Deref;
use acpi::{Handler};
use acpi::aml::Interpreter;
use spin::{Mutex, Once};
use crate::acpi::handler::TmpHandler;
use crate::memory::paging::PHY_OFFSET;
use crate::result;
use crate::result::{Error, ErrorType};

static CACHE: Once<Mutex<Interpreter<TmpHandler>>> = Once::new();

pub fn get_dsdt() -> result::Result<&'static Mutex<Interpreter<TmpHandler>>> {
    if likely(CACHE.is_completed()) {
        return Ok(unsafe{CACHE.get().unwrap_unchecked()});
    }

    let Some(table) = super::core::ACPI_TABLE_TMP_HANDLER.get() else {
        cold_path();
        return Error::new(
            ErrorType::NotInitialized,
            Some("acpi table is not initialized"),
        ).raise();
    };

    for i in table.find_tables::<acpi::sdt::fadt::Fadt>() {
        let Ok(addr) = i.get().dsdt_address() else {
            cold_path();
            return Error::new(
                ErrorType::InvalidData,
                Some("dsdt address is invalid"),
            ).raise();
        };

        let dsdt_header = unsafe { *((addr + PHY_OFFSET) as *const acpi::sdt::SdtHeader) };
        let dsdt_revision = dsdt_header.revision;

        let reg = acpi::registers::FixedRegisters::new(
            i.deref(),
            i.handler.clone()
        );

        let Ok(reg) = reg else {
            cold_path();
            return Error::new(
                ErrorType::InternalError,
                Some("failed to create fixed registers"),
            ).raise();
        };

        let Ok(addr) = i.facs_address() else {
            cold_path();
            return Error::new(
                ErrorType::InvalidData,
                Some("facs_address is invalid"),
            ).raise();
        };

        let ptr = unsafe{i.handler.map_physical_region(
            addr,
            size_of::<acpi::sdt::facs::Facs>()
        )};

        let interr = Interpreter::new(
            i.handler.clone(),
            dsdt_revision,
            Arc::new(reg),
            Some(ptr)
        );

        let val = CACHE.call_once(|| Mutex::new(interr));

        return Ok(val);
    }

    Error::new(
        ErrorType::NotFound,
        Some("fadt not found"),
    ).raise()
}