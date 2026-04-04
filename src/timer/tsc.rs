use core::hint::spin_loop;
use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;
use spin::{Lazy, RwLock};
use x86::time::rdtsc;
use crate::util::debug::with_interr;
use crate::{cpu_info};
use crate::thread_local::read_gs;
use crate::timer::{Timer, TimerConst, TimerConstTimeStampInfo};

pub static TSC: Lazy<Tsc> = Lazy::new(|| {
    Tsc::new()
});

#[derive(Debug, Default)]
pub struct TscGsData {
    pub par_100ns: u64,
    pub adjust: i64,
}

#[derive(Debug)]
pub struct Tsc {
    pub is_invariant: bool,
    pub utc_offset: RwLock<Duration>,
}

impl Tsc {
    pub fn new() -> Self {
        let is_invariant = cpu_info!(environment::tsc::InvariantTsc);
        Self {
            is_invariant,
            utc_offset: RwLock::new(Duration::default()),
        }
    }

    pub fn init_for_ap(&self, timer: fn(Duration) -> (), wait: Duration) {
        if with_interr(|| -> bool {
            let gs = read_gs().unwrap();
            if gs.tsc_init {
                return true;
            }
            false
        }) {
            return;
        }

        let (start, end) = with_interr(|| {
            let start = Self::get();
            timer(wait);
            let end = Self::get();
            (start, end)
        });

        let count = end.wrapping_sub(start);
        let units = (wait.as_nanos() / 100) as u64;

        if units == 0 { return; }
        let par_100ns_value = count / units;

        with_interr(|| {
            let gs = read_gs().unwrap();
            gs.tsc_init = true;
        });

        with_interr(|| {
            let gs = read_gs().unwrap();
            gs.tsc_data.par_100ns = par_100ns_value;
        });
    }

    pub fn get_100ns(&self) -> u64 {
        with_interr(|| {
            let gs = read_gs().unwrap();
            let common_tsc = Tsc::get() as i64 + gs.tsc_data.adjust;

            let tsc_u64 = if common_tsc < 0 { 0 } else { common_tsc as u64 };
            tsc_u64 / gs.tsc_data.par_100ns
        })
    }

    #[inline]
    pub fn get() -> u64 {
        unsafe{rdtsc()}
    }
}

impl const TimerConst for Tsc {
    fn accuracy(&self) -> Duration {
        Duration::from_nanos(100)
    }

    fn utc_supported(&self) -> TimerConstTimeStampInfo {
        TimerConstTimeStampInfo::NeedInit
    }

    fn lts_supported(&self) -> TimerConstTimeStampInfo {
        TimerConstTimeStampInfo::NeedInit
    }
}

impl Timer for Tsc {
    fn get_time(&self) -> Duration {
        let gs = read_gs().unwrap();
        if !gs.tsc_init || gs.tsc_data.par_100ns == 0 { return Duration::ZERO; }

        let common = Tsc::get() as i64 + gs.tsc_data.adjust;
        let common_u128 = if common < 0 { 0 } else { common as u128 };

        let nanos = (common_u128 * 100) / gs.tsc_data.par_100ns as u128;

        Duration::from_nanos(nanos as u64)
    }

    fn spin(&self, wait: Duration) {
        let start_tsc = Self::get() as u128;

        let counts_per_100ns = with_interr(|| {
            read_gs().unwrap().tsc_data.par_100ns
        }) as u128;

        if counts_per_100ns == 0 { return; }

        let wait_counts = (wait.as_nanos() * counts_per_100ns) / 100;

        let target_tsc = start_tsc + wait_counts;

        while (Self::get() as u128) < target_tsc {
            spin_loop();
        }
    }

    fn option_init_time_stamp(&self, utc: Duration) {
        with_interr(|| {
            let current_time = self.get_time();
            let target = utc - current_time;

            let mut lock = self.utc_offset.write();
            *lock = target;
        })
    }

    fn get_world_time_utc(&self) -> Option<Duration> {
        let offset = with_interr(|| *self.utc_offset.read());

        if offset.is_zero() {
            return None;
        }

        Some(offset + self.get_time())
    }
}