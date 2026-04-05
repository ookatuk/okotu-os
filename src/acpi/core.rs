use acpi::AcpiTables;
use spin::Once;
use uefi_raw::Guid;
use uefi_raw::table::system::SystemTable;

use uefi::guid;
use crate::acpi::handler::TmpHandler;
use crate::{result};
use crate::result::{Error, ErrorType};

/// temp acpi table handler
pub static ACPI_TABLE_TMP_HANDLER: Once<AcpiTables<TmpHandler>> = Once::new();

const ACPI_20_TABLE_GUID: Guid = guid!("882ce188-fb41-11d3-9a0d-00a0c969723b");
const ACPI_10_TABLE_GUID: Guid = guid!("eb9d2d30-2d88-11d3-9a16-0090273fc14d");

/// Init [`ACPI_TABLE_TMP_HANDLER`].
/// # Errors
/// * [`ErrorType::NotFound`] - If not found x2/x1 acpi table
///
/// # Examples
/// ```no_run
/// let st: &uefi_raw::table::system::SystemTable = unsafe{uefi::table::system_table_raw()}.unwrap().as_ref();
///
/// // ---
///
/// assert!(!ACPI_TABLE_TMP_HANDLER.completed()); // Not initialized
///
/// let res = get_acpi(st);
/// assert!(res.is_ok());
///
/// assert!(ACPI_TABLE_TMP_HANDLER.completed()); // Initialized
/// ```
pub fn get_acpi(st: &SystemTable) -> result::Result<()> {
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

    let test = Error::from_option(test, Some(ErrorType::NotFound), Some("Rsdp not found"))?;

    let res = unsafe {
        let a = TmpHandler::new();

        Error::try_raise(
            AcpiTables::from_rsdp(a, test.addr()),
            Some("Rsdp not found"),
        )?
    };

    ACPI_TABLE_TMP_HANDLER.call_once(|| {res});
    Ok(())
}