#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
compile_error!("check_mmonitor_memory supports only macOS on Apple Silicon");

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod app {
    use std::{error::Error, ffi::c_void, io, mem::MaybeUninit, process::ExitCode};

    pub fn main() -> ExitCode {
        match collect() {
            Ok(output) => {
                println!("{output}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("memory collection failed: {error}");
                ExitCode::from(3)
            }
        }
    }

    fn collect() -> Result<String, Box<dyn Error>> {
        let host = unsafe { mach2::mach_init::mach_host_self() };
        let mut stats = unsafe { std::mem::zeroed::<libc::vm_statistics64>() };
        let mut count = libc::HOST_VM_INFO64_COUNT;
        let result = unsafe {
            libc::host_statistics64(
                host,
                libc::HOST_VM_INFO64,
                (&mut stats as *mut libc::vm_statistics64).cast(),
                &mut count,
            )
        };
        if result != libc::KERN_SUCCESS {
            return Err(format!("host_statistics64 returned {result}").into());
        }

        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page_size <= 0 {
            return Err(io::Error::last_os_error().into());
        }
        let page_size = page_size as u64;
        let bytes = |pages: u64| pages.saturating_mul(page_size);

        let total: u64 = sysctl(b"hw.memsize\0")?;
        if total == 0 {
            return Err("hw.memsize returned zero".into());
        }
        let swap: libc::xsw_usage = sysctl(b"vm.swapusage\0")?;

        let free = bytes(u64::from(
            stats.free_count.saturating_sub(stats.speculative_count),
        ));
        let used = bytes(
            u64::from(
                stats
                    .internal_page_count
                    .saturating_sub(stats.purgeable_count),
            )
            .saturating_add(u64::from(stats.wire_count))
            .saturating_add(u64::from(stats.compressor_page_count)),
        );
        let available_non_compressed = bytes(
            u64::from(stats.active_count)
                .saturating_add(u64::from(stats.inactive_count))
                .saturating_add(u64::from(stats.free_count)),
        );
        let compressed = bytes(u64::from(stats.compressor_page_count));
        let available = available_non_compressed.saturating_add(compressed);
        let system_free_percent = available_non_compressed as f64 / total as f64 * 100.0;

        Ok(format!(
            "MEMORY | 'memory.total'={total}B;;;; \
             'memory.used'={used}B;;;; \
             'memory.available'={available}B;;;; \
             'memory.available_non_compressed'={available_non_compressed}B;;;; \
             'memory.free'={free}B;;;; \
             'memory.wired'={}B;;;; \
             'memory.compressed'={compressed}B;;;; \
             'memory.system_free_percent'={system_free_percent:.6}%;;;; \
             'swap.total'={}B;;;; \
             'swap.used'={}B;;;; \
             'swap.free'={}B;;;;",
            bytes(u64::from(stats.wire_count)),
            swap.xsu_total,
            swap.xsu_used,
            swap.xsu_avail,
        ))
    }

    fn sysctl<T>(name: &[u8]) -> io::Result<T> {
        let mut value = MaybeUninit::<T>::uninit();
        let mut length = size_of::<T>();
        let result = unsafe {
            libc::sysctlbyname(
                name.as_ptr().cast(),
                value.as_mut_ptr().cast::<c_void>(),
                &mut length,
                std::ptr::null_mut(),
                0,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        if length != size_of::<T>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected sysctl result size: {length}"),
            ));
        }
        Ok(unsafe { value.assume_init() })
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> std::process::ExitCode {
    app::main()
}
