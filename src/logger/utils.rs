use core::ops::{Deref, DerefMut};
use core::panic::Location;
use bincode::enc::write::Writer;
use bincode::error::EncodeError;
use uart_16550::SerialPort;
use super::core::custom_internal;

#[track_caller]
pub fn _custom(
    level: &'static str,
    by: &'static str,
    tag: &'static str,
    text: core::fmt::Arguments,
) {
    let location = Location::caller();
    custom_internal(level, by, tag, text, location);
}

pub(crate) struct UartTmp<'a>(pub &'a mut SerialPort);

impl Writer for UartTmp<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        for i in bytes.iter() {
            self.0.send_raw(*i);
        }

        Ok(())
    }
}

impl Deref for UartTmp<'_> {
    type Target = SerialPort;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for UartTmp<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}