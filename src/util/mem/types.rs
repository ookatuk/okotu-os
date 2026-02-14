use core::ops::{Add, Sub};

#[derive(Debug, Clone)]
pub struct MemMap<T = u64> {
    pub start: T,
    pub end: T,
}

impl<T> From<MemData<T>> for MemMap<T>
where
    T: Clone + Add<Output = T>
{
    fn from(value: MemData<T>) -> Self {
        Self {
            start: value.start.clone(),
            // 参照ではなく実体が必要なので、startもcloneしてから足す
            end: value.start.clone() + value.len,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemData<T = u64> {
    pub start: T,
    pub len: T,
}

impl<T> From<MemMap<T>> for MemData<T>
where
    T: Clone + Sub<Output = T>
{
    fn from(value: MemMap<T>) -> Self {
        Self {
            start: value.start.clone(),
            len: value.start - value.end,
        }
    }
}