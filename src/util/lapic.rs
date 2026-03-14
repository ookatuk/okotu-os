use crate::util::result::{self, ErrorType};
use spin::Once;
use x86_64::VirtAddr;

static LAPIC_BASE_ADDR: Once<VirtAddr> = Once::new();

const ID: usize = 0x020 / 4;
const EOI: usize = 0x0B0 / 4;
const ICR_LOW: usize = 0x300 / 4;
const ICR_HIGH: usize = 0x310 / 4;
const LVT: usize = 0x320 / 4;
const TIMER_DIV: usize = 0x3E0 / 4;
const INITIAL: usize = 0x380 / 4;

pub enum LapicOffset {
    LocalApicId,
    Eoi,
    Icr,
    LvtTimer,
    TimerDivide,
    InitialCount,
}

impl LapicOffset {
    #[inline]
    pub unsafe fn write(&self, value: u64) -> result::Result {
        #[inline(always)]
        fn option_to_res<T>(value: Option<T>) -> result::Result<T> {
            match value {
                Some(item) => Ok(item),
                None => result::Error::new(
                    ErrorType::NotInitialized,
                    Some("you need init lapic offset."),
                )
                .raise(),
            }
        }

        let addr: *mut u32 = option_to_res(get_lapic_base())?.as_mut_ptr();

        match self {
            LapicOffset::LocalApicId => {
                unsafe { addr.add(ID).write_volatile(value as u32) }

                Ok(())
            }
            LapicOffset::Eoi => {
                unsafe { addr.add(EOI).write_volatile(value as u32) }

                Ok(())
            }
            LapicOffset::Icr => {
                let low = value as u32;
                let high = (value >> 32) as u32;

                unsafe {
                    addr.add(ICR_HIGH).write_volatile(high);
                    addr.add(ICR_LOW).write_volatile(low);
                }

                Ok(())
            }
            LapicOffset::LvtTimer => {
                unsafe { addr.add(LVT).write_volatile(value as u32) }
                Ok(())
            }
            LapicOffset::TimerDivide => {
                unsafe { addr.add(TIMER_DIV).write_volatile(value as u32) }

                Ok(())
            }
            LapicOffset::InitialCount => {
                unsafe { addr.add(INITIAL).write_volatile(value as u32) }

                Ok(())
            }
        }
    }
}

pub fn init(addr: VirtAddr) -> result::Result {
    if LAPIC_BASE_ADDR.is_completed() {
        result::Error::new(
            ErrorType::AlreadyInitialized,
            Some("lapic addr is seted already."),
        )
        .raise()
    } else {
        LAPIC_BASE_ADDR.call_once(|| addr);
        Ok(())
    }
}

#[inline(always)]
pub fn get_lapic_base() -> Option<VirtAddr> {
    LAPIC_BASE_ADDR.get().map(|x| x.clone())
}
