use alloc::borrow::Cow;
use alloc::format;
use alloc::string::String;
use core::any::type_name;
use core::fmt::{Display, Formatter};

pub type Result<Output = ()> = core::result::Result<Output, Error>;

#[derive(Debug, Clone)]
pub enum ErrorType {
    GopNotFound,
    NotSupported,
    InvalidFileType,
    InvalidData,
    AllocationFailed,
    FileNotFound,
    UefiError(uefi::Error),
    OtherError,
    NotFound,
    OverFlow,
    NoMemory,
}

impl From<uefi::Error> for ErrorType {
    #[inline]
    fn from(status: uefi::Error) -> Self {
        ErrorType::UefiError(status)
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
    pub fn new(error_type: ErrorType, message: Option<&'static str>) -> Self {
        Self {
            error_type,
            message: message.map(Cow::Borrowed),
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
    pub fn raise<T>(self) -> Result<T> {
        Err(self)
    }

    #[inline]
    pub fn from_uefi(status: uefi::Error, desc: Option<&'static str>) -> Self {
        Error::new(ErrorType::from(status), desc)
    }

    #[inline]
    #[deprecated(note = "`try_raise` is easier.")]
    pub fn try_from_uefi(status: uefi::Result, desc: Option<&'static str>) -> core::result::Result<Self, ()> {
        let mut error_type = Error::try_from(status)?;

        error_type.message = desc.map(Cow::Borrowed);

        Ok(error_type)
    }

    #[inline]
    pub fn try_raise<T>(status: uefi::Result<T>, desc: Option<&'static str>) -> Result<T> {
        if status.is_ok() {
            return Ok(unsafe{status.unwrap_unchecked()});  // 大丈夫なのは確定
        }
        let error = unsafe{status.unwrap_err_unchecked()};
        Self::from_uefi(error, desc).raise()
    }

    #[inline]
    pub fn external<E: core::fmt::Debug>(err: E) -> Self {
        let msg = type_name::<E>();
        Self::new_string(ErrorType::OtherError, Some(format!("{}({:?})", msg, err)))
    }
}

impl From<uefi::Error> for Error {
    #[inline]
    fn from(status: uefi::Error) -> Self {
        Self::from_uefi(status, None)
    }
}

impl<T: core::fmt::Debug> TryFrom<uefi::Result<T>> for Error {
    type Error = ();

    #[inline]
    fn try_from(value: uefi::Result<T>) -> core::result::Result<Self, Self::Error> {
        if value.is_ok() { return Err(()); }

        Ok(Self::from(value.unwrap_err()))
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

impl core::error::Error for Error {}
unsafe impl Send for Error {}
unsafe impl Sync for Error {}