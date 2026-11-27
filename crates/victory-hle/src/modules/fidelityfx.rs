//! `amd_fidelityfx_loader_xbox.dll`: AMD FidelityFX's API, served by the
//! host's PC build of it on the host GPU when the user names a folder that
//! holds one (`VICTORY_FIDELITYFX_DIR`). Without it, or if it offers no
//! upscaler for the host GPU, this module stays out of the registry and the
//! title's own console DLLs run as before.
//!
//! Both builds have the same API (the FidelityFX SDK's, MIT): five functions
//! over chains of descriptions, each starting with a type id and the next
//! one's address; contexts are opaque pointers. The console-specific parts
//! are translated: the backend description names the console device (type
//! 0x0100_0002; on PC type 2 with the host device), and a dispatch names the
//! title's command list and resources (their host list and host copies, in
//! the states the title gives, see `victory_gpu::external`). The rest passes
//! through unchanged: sizes, flags, jitter, motion-vector scale, camera
//! values, the resource descriptions, the context. Only the upscaler: other
//! effects (frame generation) are refused.
//!
//! An explicitly configured, externally built Streamline adapter may also
//! implement this same host ABI. It is selected once before imports bind,
//! never by swapping opaque contexts or after GPU work has been recorded.
//! The adapter is not part of this crate; see docs/streamline.md.
#![allow(non_snake_case)]
// Title code calls these through its import table.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CStr, c_char, c_void};
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use victory_gpu::object::Object;
use windows::Win32::Graphics::Direct3D12::ID3D12Device;
use windows::core::Interface;

use crate::{HleModule, hle_export};

/// The PC build's upscaler, which has the five functions itself.
const DLL: &str = "amd_fidelityfx_upscaler_dx12.dll";
const STREAMLINE_DLL: &str = "victory_streamline_upscaler.dll";
const STREAMLINE_ABI: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Provider {
    Streamline,
    FidelityFx,
}

impl Provider {
    fn dll(self) -> &'static str {
        match self {
            Self::Streamline => STREAMLINE_DLL,
            Self::FidelityFx => DLL,
        }
    }
}

// Return codes.
const OK: u32 = 0;
const UNKNOWN_TYPE: u32 = 2;
const NO_PROVIDER: u32 = 4;
const PARAMETER: u32 = 6;

// Description types.
const CREATE_UPSCALE: u64 = 0x1_0000;
const DISPATCH_UPSCALE: u64 = 0x1_0001;
const CONSOLE_BACKEND: u64 = 0x100_0002;
const HOST_BACKEND: u64 = 2;
const GET_VERSIONS: u64 = 4;
const GET_PROVIDER_VERSION: u64 = 6;
const MEMORY_USAGE_WITH_DEVICE: u64 = 0x1_0009;

#[repr(C)]
#[derive(Clone, Copy)]
struct Header {
    kind: u64,
    next: *mut Header,
}

/// An upscaler context: flags, largest render and output sizes, a message
/// callback.
#[repr(C)]
#[derive(Clone, Copy)]
struct CreateUpscale {
    header: Header,
    flags: u32,
    max_render: [u32; 2],
    max_output: [u32; 2],
    message: usize,
}

/// The backend description: a device.
#[repr(C)]
struct Backend {
    header: Header,
    device: *mut c_void,
}

/// A resource, its description (type, format, sizes, mips, flags, usage)
/// and the state it is in.
#[repr(C)]
#[derive(Clone, Copy)]
struct Resource {
    resource: *mut c_void,
    description: [u32; 8],
    state: u32,
}

/// One upscaling: the command list, then colour, depth, motion vectors,
/// exposure, reactive mask, transparency mask and output; then jitter,
/// motion-vector scale, sizes, sharpening, frame time, pre-exposure, reset,
/// camera and flags, which pass through.
#[repr(C)]
#[derive(Clone, Copy)]
struct DispatchUpscale {
    header: Header,
    list: *mut c_void,
    resources: [Resource; 7],
    jitter: [f32; 2],
    motion_scale: [f32; 2],
    render: [u32; 2],
    output: [u32; 2],
    sharpen: u32,
    sharpness: f32,
    frame_ms: f32,
    pre_exposure: f32,
    reset: u32,
    near: f32,
    far: f32,
    fov: f32,
    meters: f32,
    flags: u32,
}

const _: () = assert!(size_of::<CreateUpscale>() == 0x30);
const _: () = assert!(size_of::<Resource>() == 0x30);
const _: () = assert!(size_of::<DispatchUpscale>() == 0x1b0);

/// The versions a backend offers: count in and out, ids, names.
#[repr(C)]
struct GetVersions {
    header: Header,
    create_kind: u64,
    device: *mut c_void,
    count: *mut u64,
    ids: *mut u64,
    names: *mut *const c_char,
}

#[repr(C)]
struct ProviderVersion {
    header: Header,
    id: u64,
    name: *const c_char,
}

type Create = unsafe extern "C" fn(*mut *mut c_void, *mut Header, *const c_void) -> u32;
type Destroy = unsafe extern "C" fn(*mut *mut c_void, *const c_void) -> u32;
/// Configure, query and dispatch.
type Call = unsafe extern "C" fn(*mut *mut c_void, *mut Header) -> u32;

// Keep loaded code alive for all contexts and recorded work. On a failed
// probe, the provider must have left no device work or background threads.
struct Module(windows_sys::Win32::Foundation::HMODULE);

impl Drop for Module {
    fn drop(&mut self) {
        // SAFETY: our reference from LoadLibraryExW.
        unsafe { windows_sys::Win32::Foundation::FreeLibrary(self.0) };
    }
}

struct Host {
    _module: Module,
    provider: Provider,
    create: Create,
    destroy: Destroy,
    configure: Call,
    query: Call,
    dispatch: Call,
    device: ID3D12Device,
}

// SAFETY: D3D12 devices are free-threaded; the rest is code.
unsafe impl Send for Host {}
// SAFETY: as above.
unsafe impl Sync for Host {}

static HOST: OnceLock<Host> = OnceLock::new();

macro_rules! once {
    ($level:ident, $($arg:tt)*) => {{
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| tracing::$level!($($arg)*));
    }};
}

/// Preserve the original entry point for callers that only use FidelityFX.
pub fn use_host_dir(dir: Option<&Path>) {
    use_host_dirs(None, dir);
}

/// An external Streamline adapter, if explicitly configured and capable,
/// then PC FidelityFX, then the title's own DLLs. Only startup failures
/// trigger fallback: contexts and recorded commands cannot be transferred.
pub fn use_host_dirs(streamline: Option<&Path>, fidelityfx: Option<&Path>) {
    if HOST.get().is_some() {
        return;
    }
    if let Some((provider, (host, versions))) = select_provider(streamline, fidelityfx, load) {
        tracing::info!(?provider, versions, "host upscaler serves the title's FidelityFX API");
        let _ = HOST.set(host);
    } else {
        tracing::info!("no host upscaler selected; the title's own FidelityFX DLLs run");
    }
}

fn select_provider<T>(
    streamline: Option<&Path>,
    fidelityfx: Option<&Path>,
    mut load: impl FnMut(&Path, Provider) -> Result<T, String>,
) -> Option<(Provider, T)> {
    for (provider, dir) in [(Provider::Streamline, streamline), (Provider::FidelityFx, fidelityfx)] {
        let Some(dir) = dir else { continue };
        match load(dir, provider) {
            Ok(loaded) => return Some((provider, loaded)),
            Err(why) => tracing::warn!(?provider, folder = %dir.display(), %why, "host upscaler unavailable"),
        }
    }
    None
}

/// Whether the host's FidelityFX serves the title's.
pub(crate) fn serving() -> bool {
    HOST.get().is_some()
}

fn load(dir: &Path, provider: Provider) -> Result<(Host, String), String> {
    let dll = provider.dll();
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
        LoadLibraryExW,
    };
    // The search flags below want a full path.
    let path = std::path::absolute(dir.join(dll)).map_err(|e| e.to_string())?;
    if !path.is_file() {
        return Err(format!("no {dll} there"));
    }
    let device = victory_gpu::external::host_device().ok_or("there is no host GPU")?;
    if provider == Provider::Streamline
        && victory_gpu::external::host_vendor_id(&device) != Some(0x10de)
    {
        return Err("Streamline adapter requires an NVIDIA host GPU".into());
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // Its own folder and System32 only, never the current folder.
    let search = LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32;
    // SAFETY: a NUL-terminated path to the user's FidelityFX build.
    let module = unsafe { LoadLibraryExW(wide.as_ptr(), null_mut(), search) };
    if module.is_null() {
        return Err(format!(
            "{dll} did not load ({})",
            std::io::Error::last_os_error()
        ));
    }
    let module_guard = Module(module);
    let find = |name: &CStr| {
        // SAFETY: a loaded module and a NUL-terminated name.
        unsafe { GetProcAddress(module, name.as_ptr().cast()) }
            .map(|f| f as *const c_void)
            .ok_or_else(|| format!("{dll} has no {}", name.to_string_lossy()))
    };
    if provider == Provider::Streamline {
        // SAFETY: explicitly configured adapter's versioned entry point.
        let abi = unsafe {
            std::mem::transmute::<*const c_void, unsafe extern "C" fn() -> u32>(
                find(c"victoryStreamlineBridgeAbi")?,
            )()
        };
        if abi != STREAMLINE_ABI {
            return Err(format!("unsupported Streamline adapter ABI {abi}, expected {STREAMLINE_ABI}"));
        }
    }
    // SAFETY: the API's exports, with its signatures.
    let host = unsafe {
        Host {
            _module: module_guard,
            provider,
            create: std::mem::transmute::<*const c_void, Create>(find(c"ffxCreateContext")?),
            destroy: std::mem::transmute::<*const c_void, Destroy>(find(c"ffxDestroyContext")?),
            configure: std::mem::transmute::<*const c_void, Call>(find(c"ffxConfigure")?),
            query: std::mem::transmute::<*const c_void, Call>(find(c"ffxQuery")?),
            dispatch: std::mem::transmute::<*const c_void, Call>(find(c"ffxDispatch")?),
            device,
        }
    };
    let versions = versions(&host)?;
    Ok((host, versions))
}

/// The upscaler versions the host DLL offers for the host GPU, by name; an
/// error if none.
fn versions(host: &Host) -> Result<String, String> {
    let mut count = 0u64;
    let mut query = GetVersions {
        header: Header {
            kind: GET_VERSIONS,
            next: null_mut(),
        },
        create_kind: CREATE_UPSCALE,
        device: host.device.as_raw(),
        count: &mut count,
        ids: null_mut(),
        names: null_mut(),
    };
    // SAFETY: a version query naming the host device.
    let code = unsafe { (host.query)(null_mut(), &mut query.header) };
    if code != OK || count == 0 || count > 256 {
        return Err(format!(
            "it offers no upscaler for the host GPU (code {code})"
        ));
    }
    let mut names = vec![null::<c_char>(); count as usize];
    query.names = names.as_mut_ptr();
    // SAFETY: as above, with room for `count` names.
    let code = unsafe { (host.query)(null_mut(), &mut query.header) };
    if code != OK || count == 0 || count as usize > names.len() {
        return Err(format!("provider version enumeration failed (code {code})"));
    }
    Ok(names
        .iter()
        .take(count as usize)
        .filter(|n| !n.is_null())
        // SAFETY: NUL-terminated names the DLL keeps.
        .map(|&n| unsafe { CStr::from_ptr(n) }.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(", "))
}

/// Puts the host backend after the title's upscaler description, in place
/// of the console's backend that follows it (where the title puts it);
/// anywhere else the console's is left for the host DLL to ignore, ours
/// coming first.
fn link(upscale: &mut CreateUpscale, backend: &mut Backend) {
    let after = upscale.header.next;
    // SAFETY: the title's chain.
    backend.header.next = match unsafe { after.as_ref() } {
        Some(h) if h.kind == CONSOLE_BACKEND => h.next,
        _ => after,
    };
    backend.header.kind = HOST_BACKEND;
    upscale.header.next = &raw mut backend.header;
}

/// A FidelityFX resource state as the host resource state it means (as the
/// PC backend reads it); `None` for generic read and indirect arguments,
/// which a host texture cannot be in.
fn host_state(state: u32) -> Option<u32> {
    Some(match state {
        0x1 | 0x80 => 0,  // common, present
        0x2 => 0x8,       // unordered access
        0x4 => 0x40,      // compute read
        0x8 => 0x80,      // pixel read
        0xc => 0xc0,      // both
        0x10 => 0x800,    // copy source
        0x20 => 0x400,    // copy destination
        0x100 => 0x4,     // render target
        0x200 => 0x10,    // depth write
        _ => return None, // generic read, indirect arguments
    })
}

extern "C" fn ffxCreateContext(
    context: *mut *mut c_void,
    desc: *mut Header,
    memory: *const c_void,
) -> u32 {
    let Some(host) = HOST.get() else {
        return NO_PROVIDER;
    };
    if context.is_null() || desc.is_null() {
        return PARAMETER;
    }
    // SAFETY: the title passes a chain of descriptions.
    let kind = unsafe { (*desc).kind };
    if kind != CREATE_UPSCALE {
        once!(
            warn,
            kind = format_args!("{kind:#x}"),
            "FidelityFX: only the upscaler runs on the host; another effect was refused"
        );
        return NO_PROVIDER;
    }
    // SAFETY: an upscaler description, by its type.
    let mut upscale = unsafe { desc.cast::<CreateUpscale>().read() };
    let mut backend = Backend {
        header: Header {
            kind: HOST_BACKEND,
            next: null_mut(),
        },
        device: host.device.as_raw(),
    };
    link(&mut upscale, &mut backend);
    // SAFETY: a PC chain; the title's later descriptions live for the call.
    let code = unsafe { (host.create)(context, &mut upscale.header, memory) };
    if code != OK {
        tracing::error!(code, render = ?upscale.max_render, output = ?upscale.max_output, flags = format_args!("{:#x}", upscale.flags), "FidelityFX: the host refused the title's upscaler");
        return code;
    }
    let mut version = ProviderVersion {
        header: Header {
            kind: GET_PROVIDER_VERSION,
            next: null_mut(),
        },
        id: 0,
        name: null(),
    };
    // SAFETY: a provider query on the context just made.
    unsafe { (host.query)(context, &mut version.header) };
    // SAFETY: a NUL-terminated name the DLL keeps, if any.
    let name = (!version.name.is_null())
        .then(|| unsafe { CStr::from_ptr(version.name) }.to_string_lossy());
    tracing::info!(
        backend = ?host.provider,
        provider = name.as_deref().unwrap_or("?"),
        id = format_args!("{:#x}", version.id),
        render = ?upscale.max_render,
        output = ?upscale.max_output,
        flags = format_args!("{:#x}", upscale.flags),
        "FidelityFX: upscaler context on the host"
    );
    OK
}

extern "C" fn ffxDestroyContext(context: *mut *mut c_void, memory: *const c_void) -> u32 {
    // SAFETY: the title passes its context.
    HOST.get()
        .map_or(NO_PROVIDER, |h| unsafe { (h.destroy)(context, memory) })
}

/// Settings (key-value pairs, debug messages) name nothing console-specific.
extern "C" fn ffxConfigure(context: *mut *mut c_void, desc: *mut Header) -> u32 {
    // SAFETY: the title passes its context and descriptions.
    HOST.get()
        .map_or(NO_PROVIDER, |h| unsafe { (h.configure)(context, desc) })
}

extern "C" fn ffxQuery(context: *mut *mut c_void, desc: *mut Header) -> u32 {
    let Some(host) = HOST.get() else {
        return NO_PROVIDER;
    };
    if desc.is_null() {
        return PARAMETER;
    }
    // SAFETY: the title passes a description.
    let kind = unsafe { (*desc).kind };
    // These carry the console device.
    if kind == GET_VERSIONS || kind == MEMORY_USAGE_WITH_DEVICE {
        once!(
            warn,
            kind = format_args!("{kind:#x}"),
            "FidelityFX: a query naming a device was refused"
        );
        return PARAMETER;
    }
    // SAFETY: as above.
    unsafe { (host.query)(context, desc) }
}

extern "C" fn ffxDispatch(context: *mut *mut c_void, desc: *mut Header) -> u32 {
    let Some(host) = HOST.get() else {
        return NO_PROVIDER;
    };
    if desc.is_null() {
        return PARAMETER;
    }
    // SAFETY: the title passes a description.
    let kind = unsafe { (*desc).kind };
    if kind != DISPATCH_UPSCALE {
        once!(
            warn,
            kind = format_args!("{kind:#x}"),
            "FidelityFX: only upscaling runs on the host; a dispatch was refused"
        );
        return UNKNOWN_TYPE;
    }
    // SAFETY: an upscaling, by its type.
    let mut dispatch = unsafe { desc.cast::<DispatchUpscale>().read() };
    let list = dispatch.list.cast::<Object>().cast_const();
    once!(info, backend = ?host.provider, render = ?dispatch.render, output = ?dispatch.output,
        jitter = ?dispatch.jitter, motion_scale = ?dispatch.motion_scale,
        frame_ms = dispatch.frame_ms, pre_exposure = dispatch.pre_exposure,
        reset = dispatch.reset, near = dispatch.near, far = dispatch.far, fov = dispatch.fov,
        flags = dispatch.flags, resources = ?dispatch.resources.map(|r| (!r.resource.is_null(), r.description, r.state)),
        "FidelityFX: first dispatch metadata (no texture contents)");
    // Alive for the call (the list holds them too).
    let mut textures = Vec::new();
    for r in dispatch
        .resources
        .iter_mut()
        .filter(|r| !r.resource.is_null())
    {
        let wanted = host_state(r.state);
        let texture = wanted.and_then(|w| {
            victory_gpu::external::host_texture(list, r.resource.cast::<Object>(), w)
                .filter(|&(_, state)| state == w)
        });
        let Some((texture, _)) = texture else {
            once!(
                warn,
                state = r.state,
                "FidelityFX: a resource has no host texture in its state; the upscaling did not run"
            );
            return PARAMETER;
        };
        r.resource = texture.as_raw();
        textures.push(texture);
    }
    once!(info, formats = ?textures.iter().map(|t| {
        // SAFETY: a live texture.
        unsafe { t.GetDesc() }.Format.0
    }).collect::<Vec<_>>(), "FidelityFX: first upscaling, host formats of its textures");
    victory_gpu::external::record(list, |host_list| {
        dispatch.list = host_list.as_raw();
        // SAFETY: the PC dispatch on host objects; the context is the host's.
        unsafe { (host.dispatch)(context, &raw mut dispatch.header) }
    })
    .unwrap_or(PARAMETER)
}

pub(crate) fn module() -> HleModule {
    HleModule {
        name: "fidelityfx",
        dlls: &["amd_fidelityfx_loader_xbox.dll"],
        exports: vec![
            hle_export!(ffxConfigure),
            hle_export!(ffxCreateContext),
            hle_export!(ffxDestroyContext),
            hle_export!(ffxDispatch),
            hle_export!(ffxQuery),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(kind: u64, next: *mut Header) -> Header {
        Header { kind, next }
    }

    /// The title's chain: upscaler, console backend, version.
    #[test]
    fn the_console_backend_becomes_the_hosts() {
        let mut version = header(0x1_000b, null_mut());
        let mut console = Backend {
            header: header(CONSOLE_BACKEND, &raw mut version),
            device: 0x1234 as *mut c_void,
        };
        let mut upscale = CreateUpscale {
            header: header(CREATE_UPSCALE, &raw mut console.header),
            flags: 0x29,
            max_render: [1280, 720],
            max_output: [1920, 1080],
            message: 0,
        };
        let mut backend = Backend {
            header: header(0, null_mut()),
            device: 0x5678 as *mut c_void,
        };
        link(&mut upscale, &mut backend);
        assert_eq!(upscale.header.next, &raw mut backend.header);
        assert_eq!(backend.header.kind, HOST_BACKEND);
        assert_eq!(backend.header.next, &raw mut version);
        assert_eq!(backend.device, 0x5678 as *mut c_void);
        // Without a console backend right after it, ours goes in front.
        let mut upscale = CreateUpscale {
            header: header(CREATE_UPSCALE, &raw mut version),
            ..upscale
        };
        link(&mut upscale, &mut backend);
        assert_eq!(backend.header.next, &raw mut version);
    }

    /// Without the PC build the title's own DLL serves its imports.
    #[test]
    fn without_the_dll_the_titles_own_runs() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path(), Provider::FidelityFx).err().unwrap().contains(DLL));
        assert!(load(dir.path(), Provider::Streamline).err().unwrap().contains(STREAMLINE_DLL));
        assert!(!serving());
        let symbol = victory_pe::ImportSymbol::Name {
            hint: 0,
            name: "ffxCreateContext".into(),
        };
        let resolver = crate::HleResolver::new(crate::HostPolicy::none());
        assert_eq!(
            resolver.classify("amd_fidelityfx_loader_xbox.dll", &symbol),
            None
        );
    }

    #[test]
    fn provider_selection_prefers_opt_in_streamline_and_falls_back_at_startup() {
        let sl = Path::new("sl");
        let fsr = Path::new("fsr");
        let mut attempted = Vec::new();
        let selected = select_provider(Some(sl), Some(fsr), |dir, provider| {
            attempted.push((dir.to_path_buf(), provider));
            if provider == Provider::Streamline { Err("unsupported".into()) } else { Ok(7) }
        });
        assert_eq!(selected, Some((Provider::FidelityFx, 7)));
        assert_eq!(attempted, vec![(sl.to_path_buf(), Provider::Streamline), (fsr.to_path_buf(), Provider::FidelityFx)]);
        let selected = select_provider(Some(sl), Some(fsr), |_, provider| {
            assert_eq!(provider, Provider::Streamline);
            Ok(8)
        });
        assert_eq!(selected, Some((Provider::Streamline, 8)));
        assert_eq!(select_provider::<()>(None, None, |_, _| panic!("no paths")), None);
        assert_eq!(select_provider::<()>(Some(sl), Some(fsr), |_, _| Err("missing".into())), None);
    }

    #[test]
    fn dispatch_metadata_keeps_the_verified_console_abi() {
        assert_eq!(std::mem::offset_of!(DispatchUpscale, resources), 0x18);
        assert_eq!(std::mem::offset_of!(DispatchUpscale, jitter), 0x168);
        assert_eq!(std::mem::offset_of!(DispatchUpscale, flags), 0x1ac);
    }

    #[test]
    fn resource_states_read_as_the_pc_backend_reads_them() {
        assert_eq!(host_state(0xc), Some(0xc0));
        assert_eq!(host_state(0x2), Some(0x8));
        assert_eq!(host_state(0x200), Some(0x10));
        assert_eq!(host_state(0x14), None);
    }
}
