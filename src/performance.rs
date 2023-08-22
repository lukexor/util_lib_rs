//! Performance profiling.

/// Begin performance profiling. Call this at the start of your main method or whenever you'd like
/// the profiling timestamp to begin.
#[inline]
pub fn profile_begin() {
    #[cfg(feature = "perf")]
    inner::GLOBAL_PROFILER.with(|profiler| profiler.borrow_mut().begin());
}

/// End performance profiling and print the metrics to `stderr`.
#[inline]
pub fn profile_end_and_print() {
    #[cfg(feature = "perf")]
    inner::GLOBAL_PROFILER.with(|profiler| profiler.borrow_mut().end_and_print());
}

/// Profile a given function or block of code. This macro will automatically use the fully
/// qualified function name when used without arguments. You can also optionally pass a custom name
/// for this profile block and a number of bytes for measuring bandwidth throughput.
///
/// # Examples
///
/// ```
/// use util_lib_rs::profile;
///
/// fn my_function() {
///     profile!();
///
///     for _ in 0..10000 {
///         profile!("loop");
///     }
/// }
/// ```
///
/// ```
/// use util_lib_rs::profile;
///
/// fn read_data() {
///     let bytes_read = 1000;
///     profile!("read_data", bytes_read);
/// }
/// ```
#[macro_export]
macro_rules! profile {
    () => {
        #[cfg(feature = "perf")]
        fn __f() {}
        #[cfg(feature = "perf")]
        profile!($crate::performance::inner::function_name(__f));
    };
    ($name:expr) => {
        profile!($name, 0);
    };
    ($name:expr, $byte_count:expr) => {
        #[cfg(feature = "perf")]
        let __pb = $crate::performance::inner::ProfileBlock::new($name, $byte_count);
    };
}

#[cfg(feature = "perf")]
pub mod inner {
    use std::{
        cell::RefCell,
        time::{SystemTime, UNIX_EPOCH},
    };

    thread_local! {
        /// Global profiler object for each thread which tracks start/end timestamp counters and
        /// list of profile anchors.
        pub(super) static GLOBAL_PROFILER: RefCell<Profiler> = RefCell::new(Profiler {
            start_tsc: 0,
            end_tsc: 0,
            anchors: Vec::with_capacity(4096),
            parent: None,
        });
    }

    /// Utility function to generate the name of the current function.
    #[must_use]
    pub fn function_name<T>(_: T) -> &'static str {
        let name = std::any::type_name::<T>();
        &name[..name.len() - 3]
    }

    #[derive(Debug)]
    #[must_use]
    pub(super) struct Profiler {
        start_tsc: u64,
        end_tsc: u64,
        anchors: Vec<ProfileAnchor>,
        parent: Option<&'static str>,
    }

    impl Profiler {
        pub(super) fn begin(&mut self) {
            self.start_tsc = Self::read_block_timer();
        }

        #[allow(clippy::cast_precision_loss)]
        pub(super) fn end_and_print(&mut self) {
            self.end_tsc = Self::read_block_timer();
            let timer_freq = Self::estimated_block_timer_freq();

            let elapsed_tsc = self.end_tsc - self.start_tsc;
            if elapsed_tsc > 0 {
                println!(
                    "\nTotal time: {:.4}ms (timer freq {})",
                    1000.0 * elapsed_tsc as f64 / timer_freq as f64,
                    timer_freq
                );
            }

            for anchor in &self.anchors {
                if anchor.tsc_elapsed_inclusive > 0 {
                    anchor.print_time_elapsed(elapsed_tsc, timer_freq);
                }
            }
        }

        /// Returns a conversion factor for OS timer. In the case of linux, the units are in microseconds.
        fn get_os_timer_freq() -> u64 {
            1_000_000
        }

        fn read_os_timer() -> u64 {
            let since_epoch = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time is earlier than Unix Epoch");
            Self::get_os_timer_freq() * since_epoch.as_secs()
                + u64::from(since_epoch.subsec_micros())
        }

        fn read_block_timer() -> u64 {
            let mut aux = 0;
            #[cfg(target_arch = "x86")]
            unsafe {
                std::arch::x86::__rdtscp(&mut aux)
            }
            #[cfg(target_arch = "x86_64")]
            unsafe {
                std::arch::x86_64::__rdtscp(&mut aux)
            }
            #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
            panic!("performance profiling is not supported on this architecture")
        }

        #[allow(
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss
        )]
        fn estimated_block_timer_freq() -> u64 {
            let milliseconds_to_wait = 10;
            let os_freq = Self::get_os_timer_freq();

            let block_start = Self::read_block_timer();
            let os_start = Self::read_os_timer();
            let mut os_end;
            let mut os_elapsed = 0;
            let os_wait_time = os_freq * milliseconds_to_wait / 1000;
            while os_elapsed < os_wait_time {
                os_end = Self::read_os_timer();
                os_elapsed = os_end - os_start;
            }

            let block_end = Self::read_block_timer();
            let block_elapsed = block_end - block_start;

            if os_elapsed > 0 {
                os_freq * block_elapsed / os_elapsed
            } else {
                0
            }
        }
    }

    #[derive(Debug, Default, Copy, Clone)]
    #[must_use]
    struct ProfileAnchor {
        name: &'static str,
        hit_count: u64,
        byte_count: u64,
        tsc_elapsed_exclusive: u64,
        tsc_elapsed_inclusive: u64,
    }

    impl ProfileAnchor {
        #[allow(clippy::cast_precision_loss)]
        fn print_time_elapsed(&self, elapsed_tsc: u64, timer_freq: u64) {
            let percent = 100.0 * (self.tsc_elapsed_exclusive as f64 / elapsed_tsc as f64);
            eprint!(
                "  {}[{}]: {} ({percent:.2}%",
                self.name, self.hit_count, self.tsc_elapsed_exclusive
            );
            if self.tsc_elapsed_inclusive != self.tsc_elapsed_exclusive {
                let percent_with_children =
                    100.0 * (self.tsc_elapsed_inclusive as f64 / elapsed_tsc as f64);
                eprint!(", {percent_with_children:.2}% w/children");
            }
            eprint!(")");

            if self.byte_count > 0 {
                const MB: f64 = 1024.0 * 1024.0;
                const GB: f64 = MB * 1024.0;

                let seconds = self.tsc_elapsed_exclusive as f64 / timer_freq as f64;
                let bytes_per_second = self.byte_count as f64 / seconds;
                let megabytes = self.byte_count as f64 / MB;
                let gigabytes_per_second = bytes_per_second / GB;

                eprint!("  {megabytes:.3}MB at {gigabytes_per_second:.2}GB/s");
            }

            eprintln!();
        }
    }

    /// Profile block is created inside each function scope where `profile!()` is called, keeping
    /// track of it's parent (if any), byte count, and previous elapsed timestamp counter
    /// (inclusive) in order to add up repeat calls to the same block.
    #[derive(Debug)]
    #[must_use]
    pub struct ProfileBlock {
        name: &'static str,
        parent: Option<&'static str>,
        prev_tsc_elapsed_inclusive: u64,
        start_tsc: u64,
    }

    impl ProfileBlock {
        /// Creates a new profile block which will get dropped at the end of the current scope.
        pub fn new(name: &'static str, byte_count: u64) -> Self {
            let (parent, prev_tsc_elapsed_inclusive) = GLOBAL_PROFILER.with(|profiler| {
                let mut profiler = profiler.borrow_mut();
                let parent = profiler.parent;
                profiler.parent = Some(name);
                let anchor = if let Some(anchor) = profiler
                    .anchors
                    .iter_mut()
                    .find(|anchor| anchor.name == name)
                {
                    anchor
                } else {
                    profiler.anchors.push(ProfileAnchor::default());
                    profiler
                        .anchors
                        .last_mut()
                        .expect("last item is valid since we just pushed")
                };
                anchor.name = name;
                anchor.byte_count += byte_count;
                (parent, anchor.tsc_elapsed_inclusive)
            });

            Self {
                name,
                parent,
                prev_tsc_elapsed_inclusive,
                start_tsc: Profiler::read_block_timer(),
            }
        }
    }

    impl Drop for ProfileBlock {
        /// When the `ProfileBlock` is dropped, it will calculate the total elapsed timestamp
        /// counter and update the matching `ProfileAnchor`.
        fn drop(&mut self) {
            let elapsed = Profiler::read_block_timer() - self.start_tsc;

            GLOBAL_PROFILER.with(|profiler| {
                let mut profiler = profiler.borrow_mut();
                profiler.parent = self.parent;

                if let Some(parent) = self.parent {
                    let parent = profiler
                        .anchors
                        .iter_mut()
                        .find(|anchor| anchor.name == parent)
                        .expect("valid parent anchor");
                    parent.tsc_elapsed_exclusive =
                        parent.tsc_elapsed_exclusive.saturating_sub(elapsed);
                }

                let anchor = profiler
                    .anchors
                    .iter_mut()
                    .find(|anchor| anchor.name == self.name)
                    .expect("valid anchor");
                anchor.tsc_elapsed_exclusive += elapsed;
                anchor.tsc_elapsed_inclusive = self.prev_tsc_elapsed_inclusive + elapsed;
                anchor.hit_count += 1;
            });
        }
    }
}

#[cfg(all(test, feature = "perf"))]
mod tests {
    use super::*;
    use std::hint::black_box;

    fn expensive() {
        black_box((0..1_000_000).fold(0, |acc, out| black_box(acc) ^ black_box(out)));
    }

    fn tfn2() {
        profile!();
        expensive();
    }

    fn tfn() {
        profile!();
        std::thread::sleep(std::time::Duration::from_millis(100));
        for _ in 0..5 {
            profile!("inner", 500_000);
            tfn2();
        }
    }

    #[test]
    fn profile_block() {
        profile_begin();

        for _ in 0..5 {
            tfn();
        }

        profile_end_and_print();
    }
}
