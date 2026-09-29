use crate::{Error, Result};

pub fn supported() -> Result<()> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Ok(());
    }
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        os => os,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        arch => arch,
    };
    Err(Error::Local(format!(
        "local mode needs macOS on Apple Silicon, this is {os}/{arch}"
    )))
}
pub fn usable_mib() -> Result<i64> {
    supported()?;
    if let Ok(limit) = wired_limit_mib()
        && limit > 0
    {
        return Ok(limit);
    }
    let mem = mem_bytes().map_err(|e| Error::Local(format!("sysctl hw.memsize: {e}")))?;
    Ok(((mem * 3 / 4) >> 20) as i64)
}
#[cfg(target_os = "macos")]
fn sysctl_bytes(name: &str) -> Result<Vec<u8>> {
    use std::{ffi::CString, ptr};
    let name = CString::new(name).map_err(|e| Error::Local(e.to_string()))?;
    let mut size = 0;
    // Both calls use a valid NUL-terminated name; the second owns a buffer of
    // the size returned by the first. No value is written to the sysctl.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            ptr::null_mut(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut bytes = vec![0u8; size];
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            bytes.as_mut_ptr().cast(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    bytes.truncate(size);
    Ok(bytes)
}
#[cfg(not(target_os = "macos"))]
fn sysctl_bytes(_: &str) -> Result<Vec<u8>> {
    Err(Error::Local("sysctl: darwin only".into()))
}
pub fn mem_bytes() -> Result<u64> {
    let bytes = sysctl_bytes("hw.memsize")?;
    Ok(u64::from_ne_bytes(bytes.try_into().map_err(|_| {
        Error::Local("hw.memsize is not uint64".into())
    })?))
}
pub fn wired_limit_mib() -> Result<i64> {
    let bytes = sysctl_bytes("iogpu.wired_limit_mb")?;
    Ok(i64::from(u32::from_ne_bytes(bytes.try_into().map_err(
        |_| Error::Local("iogpu.wired_limit_mb is not uint32".into()),
    )?)))
}
pub fn sysctl_string(name: &str) -> Result<String> {
    let bytes = sysctl_bytes(name)?;
    Ok(String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supported_matches_target() {
        assert_eq!(
            supported().is_ok(),
            cfg!(all(target_os = "macos", target_arch = "aarch64"))
        );
    }
    #[test]
    fn usable_mib_on_apple_silicon() {
        if supported().is_ok() {
            assert!(usable_mib().unwrap() >= 1024);
        }
    }
}
