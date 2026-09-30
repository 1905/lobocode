//! Admission policy for the local Qwen runtime. All measurements remain bytes.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type MemoryProbe = Arc<dyn Fn() -> Result<MemorySnapshot> + Send + Sync>;
const GIB: u64 = 1 << 30;
const SYSTEM_RESERVE: u64 = 4 * GIB;
const RUNTIME_RESERVE: u64 = 4 * GIB;
const KV_BYTES_PER_TOKEN: u64 = 34_816;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub metal_limit_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryAssessment {
    pub model: String,
    pub ctx: i64,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub metal_limit_bytes: u64,
    pub required_bytes: u64,
    pub budget_bytes: u64,
}
impl MemoryAssessment {
    pub fn fits(&self) -> bool {
        self.required_bytes <= self.budget_bytes
    }
    pub fn ensure_fit(&self) -> Result<()> {
        if self.fits() {
            Ok(())
        } else {
            Err(Error::Local(self.message()))
        }
    }
    pub fn message(&self) -> String {
        format!(
            "{} with {} context tokens requires {:.1} GiB; {:.1} GiB is available after reserves. Close other apps or use Cloud.",
            self.model,
            self.ctx,
            self.required_bytes as f64 / GIB as f64,
            self.budget_bytes as f64 / GIB as f64
        )
    }
}
/// Read native memory counters without opening a model or starting a process.
pub fn snapshot() -> Result<MemorySnapshot> {
    super::platform::supported()?;
    native_snapshot()
}

pub fn assess(model: &str, ctx: i64, snapshot: &MemorySnapshot) -> Result<MemoryAssessment> {
    if !matches!(model, "q6" | "q8") {
        return Err(Error::Local(format!(
            "unknown local memory profile {model:?}; select q6 or q8"
        )));
    }
    if !(1..=262_144).contains(&ctx) {
        return Err(Error::Local(format!(
            "{model} context must be 1–262144 tokens, got {ctx}"
        )));
    }
    validate(snapshot)?;
    let weights = lobo_proto::catalog::get(model).map_err(|e| Error::Local(e.to_string()))?;
    let weights =
        u64::try_from(weights.size).map_err(|_| invalid("invalid catalog weight size"))?;
    let padded = (ctx as u64).checked_add(255).ok_or_else(overflow)? / 256 * 256;
    let required = padded
        .checked_mul(KV_BYTES_PER_TOKEN)
        .and_then(|kv| weights.checked_add(kv))
        .and_then(|bytes| bytes.checked_add(RUNTIME_RESERVE))
        .ok_or_else(overflow)?;
    Ok(MemoryAssessment {
        model: model.into(),
        ctx,
        total_bytes: snapshot.total_bytes,
        available_bytes: snapshot.available_bytes,
        metal_limit_bytes: snapshot.metal_limit_bytes,
        required_bytes: required,
        budget_bytes: snapshot
            .available_bytes
            .saturating_sub(SYSTEM_RESERVE)
            .min(snapshot.metal_limit_bytes),
    })
}

pub fn inspect(model: &str, ctx: i64) -> Result<MemoryAssessment> {
    let measured = snapshot().map_err(|error| measurement_error(model, ctx, error))?;
    assess(model, ctx, &measured)
}

/// Apply the same policy to an explicitly injected native probe.
pub fn inspect_with(model: &str, ctx: i64, probe: &MemoryProbe) -> Result<MemoryAssessment> {
    let measured = probe().map_err(|error| measurement_error(model, ctx, error))?;
    assess(model, ctx, &measured).map_err(|error| measurement_error(model, ctx, error))
}
fn measurement_error(model: &str, ctx: i64, error: Error) -> Error {
    Error::Local(format!(
        "Cannot assess Mac memory for {model} with {ctx} context tokens: {error}. Close other apps and retry, or use Cloud."
    ))
}
fn invalid(detail: &str) -> Error {
    Error::Local(format!("Mac memory measurement unavailable: {detail}"))
}
fn overflow() -> Error {
    invalid("counter overflow")
}
fn validate(snapshot: &MemorySnapshot) -> Result<()> {
    if snapshot.total_bytes == 0 || snapshot.metal_limit_bytes == 0 {
        return Err(invalid("physical memory or Metal recommendation is zero"));
    }
    if snapshot.available_bytes > snapshot.total_bytes {
        return Err(invalid("available memory exceeds physical memory"));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn from_pages(
    total: u64,
    page_size: u64,
    anonymous: u64,
    purgeable: u64,
    wired: u64,
    compressor: u64,
    metal: u64,
) -> Result<MemorySnapshot> {
    if page_size == 0 || !page_size.is_power_of_two() {
        return Err(invalid("invalid page size"));
    }
    let used = anonymous
        .checked_sub(purgeable)
        .and_then(|pages| pages.checked_add(wired))
        .and_then(|pages| pages.checked_add(compressor))
        .and_then(|pages| pages.checked_mul(page_size))
        .ok_or_else(|| invalid("invalid page counters or overflow"))?;
    let available = total
        .checked_sub(used)
        .ok_or_else(|| invalid("used memory exceeds physical memory"))?;
    let snapshot = MemorySnapshot {
        total_bytes: total,
        available_bytes: available,
        metal_limit_bytes: metal,
    };
    validate(&snapshot)?;
    Ok(snapshot)
}

#[cfg(not(target_os = "macos"))]
fn native_snapshot() -> Result<MemorySnapshot> {
    Err(invalid("native probe requires macOS"))
}

#[cfg(target_os = "macos")]
fn native_snapshot() -> Result<MemorySnapshot> {
    use std::mem::{offset_of, size_of};
    // libc provides the statistics layout but not these Mach entry points.
    unsafe extern "C" {
        fn host_page_size(host: libc::host_t, size: *mut libc::vm_size_t) -> libc::kern_return_t;
        fn mach_port_deallocate(
            task: libc::mach_port_t,
            name: libc::mach_port_t,
        ) -> libc::kern_return_t;
    }
    struct HostPort(libc::mach_port_t);
    impl Drop for HostPort {
        fn drop(&mut self) {
            // mach_host_self gives this caller a send right. Release that right,
            // including every early-return path. mach_task_self is borrowed.
            unsafe {
                mach_port_deallocate(libc::mach_task_self(), self.0);
            }
        }
    }
    let total = super::platform::mem_bytes()?;
    // Obtaining a send right does not change the host's memory configuration.
    let port = unsafe { libc::mach_host_self() };
    if port == libc::MACH_PORT_NULL as libc::mach_port_t {
        return Err(invalid("Mach host port unavailable"));
    }
    let host = HostPort(port);
    let mut page_size: libc::vm_size_t = 0;
    // The host right is live and page_size points to writable storage.
    let status = unsafe { host_page_size(host.0, &mut page_size) };
    if status != libc::KERN_SUCCESS {
        return Err(invalid(&format!("host_page_size returned {status}")));
    }
    // This C structure contains only integer counters; zero is valid for all.
    let mut stats: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    // The buffer has HOST_VM_INFO64_COUNT integers. The kernel reports how
    // much it actually wrote, which can be shorter on an older OS revision.
    let status = unsafe {
        libc::host_statistics64(
            host.0,
            libc::HOST_VM_INFO64,
            (&mut stats as *mut libc::vm_statistics64).cast(),
            &mut count,
        )
    };
    if status != libc::KERN_SUCCESS {
        return Err(invalid(&format!("host_statistics64 returned {status}")));
    }
    let minimum = (offset_of!(libc::vm_statistics64, internal_page_count)
        + size_of::<libc::natural_t>())
        / size_of::<libc::integer_t>();
    if (count as usize) < minimum {
        return Err(invalid("Mach statistics omit required counters"));
    }
    let device = objc2_metal::MTLCreateSystemDefaultDevice()
        .ok_or_else(|| invalid("Metal device unavailable"))?;
    // The bindings return a Retained device; dropping it releases the handle.
    let metal = device.recommendedMaxWorkingSetSize();
    from_pages(
        total,
        page_size as u64,
        u64::from(stats.internal_page_count),
        u64::from(stats.purgeable_count),
        u64::from(stats.wire_count),
        u64::from(stats.compressor_page_count),
        metal,
    )
}

#[cfg(test)]
mod tests;
