use crate::table::TmpHandler;
use crate::util::result;
use crate::util::result::Error;
use crate::util::result::ErrorType;
use acpi::AcpiTables;
use uefi_raw::Guid;
use uefi_raw::table::system::SystemTable;

use uefi::guid;

pub const ACPI_20_TABLE_GUID: Guid = guid!("882ce188-fb41-11d3-9a0d-00a0c969723b");
pub const ACPI_10_TABLE_GUID: Guid = guid!("eb9d2d30-2d88-11d3-9a16-0090273fc14d");

pub fn get_apic_use_tmp_handler(st: &SystemTable) -> result::Result<AcpiTables<TmpHandler>> {
    let tables = unsafe {
        core::slice::from_raw_parts(
            st.configuration_table,
            st.number_of_configuration_table_entries,
        )
    };

    let mut test: Option<*mut u8> = None;

    for table in tables {
        if table.vendor_guid == ACPI_20_TABLE_GUID {
            let rsdp_ptr = table.vendor_table as *mut u8;
            test = Some(rsdp_ptr);
            break;
        }

        if table.vendor_guid == ACPI_10_TABLE_GUID {
            let rsdp_ptr = table.vendor_table as *mut u8;
            test = Some(rsdp_ptr);
        }
    }

    let test = Error::from_option(test, ErrorType::NotFound, Some("Rsdp not found"))?;

    let res = unsafe {
        let a = TmpHandler::new();

        Error::try_raise(
            AcpiTables::from_rsdp(a, test.addr()),
            Some("Rsdp not found"),
        )?
    };

    Ok(res)
}
