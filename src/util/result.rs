use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use core::any::{type_name, Any, TypeId};
use core::fmt::{Debug, Display, Formatter};

pub type Result<Output = ()> = core::result::Result<Output, Error>;

#[derive(Debug, Clone)]
pub enum ErrorType {
    NotSupported,
    InvalidFileType,
    InvalidData,
    AllocationFailed,
    FileNotFound,
    UefiError(uefi::Error),
    OtherError,
    Other(Arc<dyn Debug>),
    NotFound,
    OverFlow,
    InternalError,
    UefiBroken,
    DeviceError,
    ReadError,
    NotAFile,
    AcpiError(acpi::AcpiError),
    IndexMax,
    AlreadyUsed,
}

impl ErrorType {
    #[inline]
    const fn from_uefi(status: uefi::Error) -> Self {
        ErrorType::UefiError(status)
    }

    #[inline]
    const fn from_acpi(status: acpi::AcpiError) -> Self {
        ErrorType::AcpiError(status)
    }
}

impl Display for ErrorType {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}


unsafe impl Send for ErrorType {}
unsafe impl Sync for ErrorType {}

#[derive(Debug, Clone)]
pub struct Error {
    pub error_type: ErrorType,
    pub message: Option<Cow<'static, str>>,
}

impl Error {
    #[inline]
    pub const fn new(error_type: ErrorType, message: Option<&'static str>) -> Self {
        let message = match message {
            Some(s) => Some(Cow::Borrowed(s)),
            None => None,
        };
        Self {
            error_type,
            message,
        }
    }

    #[inline]
    pub fn new_string(error_type: ErrorType, message: Option<String>) -> Self {
        Self {
            error_type,
            message: message.map(Cow::Owned),
        }
    }

    #[inline]
    pub const fn raise<T>(self) -> Result<T> {
        Err(self)
    }

    #[inline]
    pub const fn from_uefi(status: uefi::Error, desc: Option<&'static str>) -> Self {
        Error::new(ErrorType::from_uefi(status), desc)
    }

    #[inline]
    pub const fn from_acpi(status: acpi::AcpiError, desc: Option<&'static str>) -> Self {
        Error::new(ErrorType::from_acpi(status), desc)
    }

    pub fn try_raise<T, E: 'static + Debug>(status: core::result::Result<T, E>, desc: Option<&'static str>) -> Result<T> {
        match status {
            Ok(val) => Ok(val),
            Err(error) => {
                let any_err = &error as &dyn Any;

                if let Some(acpi_err) = any_err.downcast_ref::<acpi::AcpiError>() {

                    Self::from_acpi(acpi_err.clone(), desc).raise()

                } else if let Some(uefi_err) = any_err.downcast_ref::<uefi::Error>() {

                    Self::from_uefi(uefi_err.clone(), desc).raise()

                } else if let Some(me) = any_err.downcast_ref::<Error>() {

                    me.clone().raise()

                } else {
                    let error: Arc<dyn Debug> = Arc::new(error);

                    Err(Self::new(
                        ErrorType::Other(error),
                        desc
                    ))
                }
            }
        }
    }
}

impl From<uefi::Error> for Error {
    #[inline]
    fn from(status: uefi::Error) -> Self {
        Self::from_uefi(status, None)
    }
}

impl Display for Error {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match &self.message {
            Some(msg) => write!(f, "[{:?}] {}", self.error_type, msg),
            None => write!(f, "[{:?}] (no message)", self.error_type),
        }
    }
}

impl From<Error> for Box<rhai::EvalAltResult> {
    fn from(err: Error) -> Self {
        Box::new(rhai::EvalAltResult::ErrorSystem(
            format!("{}", err),
            Box::new(err)
        ))
    }
}

impl core::error::Error for Error {}
unsafe impl Send for Error {}
unsafe impl Sync for Error {}

impl core_error::Error for Error {}