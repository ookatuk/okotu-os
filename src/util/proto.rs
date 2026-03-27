use uefi::boot;
use uefi::boot::{OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::proto::ProtocolPointer;
use crate::result;
use crate::result::{Error, ErrorType};

pub fn open<P: ProtocolPointer + ?Sized>(index: Option<usize>) -> result::Result<ScopedProtocol<P>> {
    let handles = Error::try_raise(
        boot::locate_handle_buffer(SearchType::ByProtocol(&P::GUID)),
        Some("Failed to get handle buffer")
    )?;

    let target_index = index.unwrap_or(0);
    let target_handle = *handles.get(target_index).ok_or_else(|| Error::new(
        ErrorType::NotFound,
        Some("The requested protocol handle index is out of bounds"),
    ))?;

    let protocol = unsafe {
        Error::try_raise(
            boot::open_protocol::<P>(
                OpenProtocolParams {
                    handle: target_handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            ),
            Some("Failed to open protocol")
        )?
    };

    Ok(protocol)
}