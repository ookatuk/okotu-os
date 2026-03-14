use core::{
    fmt::Debug,
    ops::{Add, Sub},
};

use rhai::CustomType;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemMap<T: Debug = u64> {
    pub start: T,
    pub end: T,
}

impl<T> From<MemData<T>> for MemMap<T>
where
    T: Clone + Add<Output = T> + Debug,
{
    fn from(value: MemData<T>) -> Self {
        Self {
            start: value.start.clone(),
            end: value.start.clone() + value.len,
        }
    }
}

impl<T> CustomType for MemMap<T>
where
    T: Debug + Clone + Send + Sync + 'static + From<u64> + Into<u64>,
{
    fn build(mut builder: rhai::TypeBuilder<Self>) {
        builder.with_get_set(
            "start",
            |me: &mut Self| -> i64 {
                let val: u64 = me.start.clone().into();
                val as i64
            },
            |me: &mut Self, value: i64| -> () {
                me.start = T::from(value as u64);
            },
        );
        builder.with_get_set(
            "end",
            |me: &mut Self| -> i64 {
                let val: u64 = me.end.clone().into();
                val as i64
            },
            |me: &mut Self, value: i64| -> () {
                me.end = T::from(value as u64);
            },
        );
    }
}

impl<T: Debug + PartialOrd + Clone> MemMap<T> {
    pub fn normalize(&mut self) {
        if self.start > self.end {
            let buf = self.start.clone();
            self.start = self.end.clone();
            self.end = buf;
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemData<T: Debug = u64> {
    pub start: T,
    pub len: T,
}

impl<T> From<MemMap<T>> for MemData<T>
where
    T: Clone + Sub<Output = T> + Debug,
{
    fn from(value: MemMap<T>) -> Self {
        Self {
            start: value.start.clone(),
            len: value.end - value.start,
        }
    }
}

impl<T> CustomType for MemData<T>
where
    T: Debug + Clone + Send + Sync + 'static + From<u64> + Into<u64>,
{
    fn build(mut builder: rhai::TypeBuilder<Self>) {
        builder.with_get_set(
            "start",
            |me: &mut Self| -> i64 {
                let val: u64 = me.start.clone().into();
                val as i64
            },
            |me: &mut Self, value: i64| -> () {
                me.start = T::from(value as u64);
            },
        );
        builder.with_get_set(
            "len",
            |me: &mut Self| -> i64 {
                let val: u64 = me.len.clone().into();
                val as i64
            },
            |me: &mut Self, value: i64| -> () {
                me.len = T::from(value as u64);
            },
        );
    }
}
