use crate::util::result;
use crate::util::result::{Error, ErrorType};
use core::hint::spin_loop;
use core::num::NonZeroU64;
use core::time::Duration;
use spin::RwLock;
use x86_64::instructions::interrupts::without_interrupts;

pub static TSC: RwLock<Tsc> = RwLock::new(Tsc::new());

#[derive(Debug, Default)]
pub struct Tsc {
    pub clock_in_100ms: Option<NonZeroU64>,
}

impl Tsc {
    pub const fn new() -> Self {
        Tsc { clock_in_100ms: None }
    }

    #[inline]
    pub fn now_clock(&self) -> u64 {
        Self::now_clock_()
    }

    #[inline]
    pub fn now_clock_() -> u64 {
        unsafe { core::arch::x86_64::_rdtsc() }
    }

    pub fn init(&mut self, timer_100ms: Option<fn()>) -> result::Result {
        let (start, end) = without_interrupts(|| {
            let start = unsafe { core::arch::x86_64::_rdtsc() };

            match timer_100ms {
                Some(timer) => timer(),
                None => uefi::boot::stall(Duration::from_millis(100)),
            }

            let end = unsafe { core::arch::x86_64::_rdtsc() };
            (start, end)
        });

        if let Some(clock) = NonZeroU64::new(end - start) {
            self.clock_in_100ms = Some(clock);
            Ok(())
        } else {
            Error::new(
                ErrorType::DeviceError,
                Some("100ms clock is zero")
            ).raise()
        }
    }

    pub fn spin_loop(&self, time: Duration) {
        let start = self.now_clock();

        let hz = self.clock_in_100ms.unwrap().get() * 10;

        let ticks_to_wait = (time.as_nanos() * hz as u128 / 1_000_000_000) as u64;

        while self.now_clock().wrapping_sub(start) < ticks_to_wait {
            spin_loop();
        }
    }
}