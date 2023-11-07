#[cfg(feature = "fullv")]
pub use fv_common::Timestamp;

#[cfg(not(feature = "fullv"))]
mod inner {
    use std::fmt;
    use std::ops::Sub;
    use std::time::Duration;

    /// 一个代表高精度时间戳的类型。
    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd)]
    pub struct Timestamp(u64);

    impl Timestamp {
        /// 返回当前世界时间的时戳。
        pub fn now_realtime() -> Self {
            Self(realtime_micros())
        }

        /// 返回当前恒增时间的时戳。
        pub fn now_monotonic() -> Self {
            Self(monotonic_micros())
        }

        /// 返回此时戳包含的微秒数。
        pub fn as_micros(&self) -> u64 {
            self.0
        }

        /// 返回此时戳包含的毫秒数。
        pub fn as_millis(&self) -> u64 {
            self.0 / 1_000
        }

        /// 返回此时戳包含的纳秒数。
        pub fn as_nanos(&self) -> u128 {
            self.0 as u128 * 1_000
        }

        /// 返回此时戳包含的秒数。
        pub fn as_secs(&self) -> u64 {
            self.0 / 1_000_000
        }
    }

    impl From<u64> for Timestamp {
        fn from(val: u64) -> Self {
            Self(val)
        }
    }

    impl From<Timestamp> for u64 {
        fn from(val: Timestamp) -> Self {
            val.0
        }
    }

    // impl fmt::Debug for Timestamp {
    //     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    //         write!(f, "{:?}", Duration::from_micros(self.0))
    //     }
    // }

    impl fmt::Display for Timestamp {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl Sub for Timestamp {
        type Output = Duration;

        fn sub(self, other: Self) -> Self::Output {
            Duration::from_micros(self.0.saturating_sub(other.0))
        }
    }

    #[cfg(target_os = "linux")]
    fn clock_gettime_micros(clock_id: libc::clockid_t) -> u64 {
        use libc::{clock_gettime, timespec};

        unsafe {
            let mut ts = timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            let _r = clock_gettime(clock_id, &mut ts);
            (ts.tv_sec as u64) * 1_000_000 + (ts.tv_nsec / 1_000) as u64
        }
    }

    #[cfg(target_os = "linux")]
    fn realtime_micros() -> u64 {
        clock_gettime_micros(libc::CLOCK_REALTIME)
    }

    #[cfg(target_os = "windows")]
    fn realtime_micros() -> u64 {
        std::time::UNIX_EPOCH
            .elapsed()
            .map(|x| x.as_micros() as u64)
            .unwrap_or(0)
    }

    #[cfg(target_os = "linux")]
    fn monotonic_micros() -> u64 {
        clock_gettime_micros(libc::CLOCK_MONOTONIC)
    }

    #[cfg(target_os = "windows")]
    fn monotonic_micros() -> u64 {
        use winapi::um::profileapi::{QueryPerformanceCounter, QueryPerformanceFrequency};
        use winapi::um::winnt::LARGE_INTEGER;
        unsafe {
            let mut counter: LARGE_INTEGER = std::mem::zeroed();
            let mut frequency: LARGE_INTEGER = std::mem::zeroed();
            QueryPerformanceCounter(&mut counter);
            QueryPerformanceFrequency(&mut frequency);
            ((*counter.QuadPart() as i128 * 1_000_000) / (*frequency.QuadPart() as i128)) as u64
        }
    }
}

#[cfg(not(feature = "fullv"))]
pub use inner::Timestamp;
