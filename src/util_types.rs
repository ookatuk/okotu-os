use core::alloc::{GlobalAlloc, Layout};
use core::hint::unlikely;
use core::ops::{Add, Div, Rem, Sub};
use num_traits::{FromPrimitive, ToPrimitive, Unsigned, Zero};

pub trait CanRangeData:
Add<Output = Self> +
Sub<Output = Self> +
Ord +
PartialOrd +
Copy +
ToPrimitive +
FromPrimitive +
Unsigned
{}

impl CanRangeData for usize {}
impl CanRangeData for u8 {}
impl CanRangeData for u16 {}
impl CanRangeData for u32 {}
impl CanRangeData for u64 {}
impl CanRangeData for u128 {}

pub struct SmartPtr<DT, GA> where
    DT: CanRangeData + Rem<Output = DT>,
    GA: GlobalAlloc + 'static,
{
    pub range: MemRangeData<DT>,
    pub alloc: &'static GA,
    align: usize,
}

impl<DT, GA> SmartPtr<DT, GA> where
    DT: CanRangeData + Div<Output = DT> + Rem<Output = DT>,
    GA: GlobalAlloc,
{
    pub fn new(ptr: usize, layout: Layout, alloc: &'static GA) -> Option<Self> {
        if unlikely(ptr.is_zero() || layout.size().is_zero()) {
            return None;
        }

        let range = MemRangeData {
            start: DT::from_usize(ptr)?,
            len: DT::from_usize(layout.size())?,
        };

        Some(Self {
            range,
            alloc,
            align: layout.align(),
        })
    }

    #[inline]
    pub const fn get_addr(&self) -> DT {
        self.range.start()
    }

    #[inline]
    pub fn get_ptr<TY>(&self) -> Option<*const TY> {
        Some(self.range.start().to_usize()? as *const TY)
    }

    pub fn get_slice<TY>(&self) -> Option<&[TY]> {
        let tsiz_raw = size_of::<TY>();
        if tsiz_raw == 0 {
            return None;
        }

        let size = DT::from_usize(tsiz_raw)?;
        let zero = DT::from_u64(0).unwrap();

        if self.range.len() % size != zero {
            return None;
        }

        let count = self.range.len() / size;

        unsafe {
            Some(core::slice::from_raw_parts(
                self.range.start().to_usize()? as *const TY,
                count.to_usize()?
            ))
        }
    }

    #[inline]
    pub fn get_mut_ptr<TY>(&mut self) -> *mut TY {
        self.range.start().to_usize().unwrap() as *mut TY
    }

    pub fn get_mut_slice<TY>(&mut self) -> Option<&mut [TY]> {
        let tsiz_raw = size_of::<TY>();
        if tsiz_raw == 0 {
            return None;
        }

        let size = DT::from_usize(tsiz_raw)?;
        let zero = DT::from_u64(0).unwrap();

        if self.range.len() % size != zero {
            return None;
        }

        let count = self.range.len() / size;

        unsafe {
            Some(core::slice::from_raw_parts_mut(
                self.range.start().to_usize()? as *mut TY,
                count.to_usize()?
            ))
        }
    }
}

impl<DT, GA> Drop for SmartPtr<DT, GA> where
    DT: CanRangeData + Div<Output = DT> + Rem<Output = DT>,
    GA: GlobalAlloc,
{
    fn drop(&mut self) {
        let size = self.range.len().to_usize().unwrap();
        let align = self.align;

        unsafe {
            self.alloc.dealloc(
                self.get_mut_ptr::<u8>(),
                Layout::from_size_align_unchecked(size, align)
            );
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemRangeData<T> where
    T: CanRangeData
{
    start: T,
    len: T
}

impl<T> MemRangeData<T> where
    T: CanRangeData
{
    #[inline]
    pub const fn new(start: T, len: T) -> MemRangeData<T> {
        MemRangeData {
            start,
            len
        }
    }

    #[inline]
    pub fn new_start_end(start: T, end: T) -> Option<MemRangeData<T>> {
        if unlikely(end > start) {
            return None;
        }

        Some(MemRangeData {
            start,
            len: end - start
        })
    }

    #[inline]
    pub const fn len(&self) -> T {
        self.len
    }

    #[inline]
    pub const fn start(&self) -> T {
        self.start
    }

    #[inline]
    pub fn end(&self) -> T {
        self.start + self.len
    }

    #[inline]
    pub fn set_start(&mut self, start: T) {
        self.start = start;
    }

    #[inline]
    pub fn set_end(&mut self, end: T) -> bool {
        if unlikely(end < self.start) {
            return false;
        }
        self.len = end - self.start;
        true
    }

    #[inline]
    pub fn set_len(&mut self, len: T) {
        self.len = len;
    }
}