# Optional NVIDIA upscaler provider (phase 1)

VIctory intercepts the title's Xbox FidelityFX API and supplies native
D3D12 objects to a PC provider. This change adds an optional, versioned
provider slot for a future NVIDIA adapter. It does not initialize Streamline,
call NGX, or execute DLSS itself.

## Configuration

In your own git-ignored `victory.toml`:

```toml
[paths]
fidelityfx_dir = 'D:\victory-data\fidelityfx'
streamline_dir = 'D:\victory-data\streamline'
```

The equivalent override is `VICTORY_STREAMLINE_DIR`. Like the other
path settings, an empty environment value is ignored. Omit the path to
keep the old behavior. Keep all SDK files and user-built DLLs outside the
repository. `victory config` reports the configured folder.

The NVIDIA folder must contain a user-built
`victory_streamline_upscaler.dll`. **No such DLL is shipped or built by
this PR.** A directory containing only the official Streamline SDK
(`sl.interposer.dll`, `sl.dlss.dll`, `nvngx_dlss.dll`) does not implement
the provider ABI. Do not rename those files to the adapter name.

Selection is once at startup, before imports bind:

1. Explicitly configured Streamline adapter, only if the actual host D3D12
   adapter is NVIDIA (PCI vendor ID 0x10de), ABI matches and provider query
   reports a usable upscaler.
2. The existing PC FidelityFX provider, if configured and supported.
3. The title's original FidelityFX DLLs.

A successful selection is not a promise that every context or dispatch will
succeed. No mid-frame retry, context switching or fallback after recording
GPU work. If the experimental adapter fails after startup, remove its path
and restart with the normal provider. Use only trusted DLLs: they execute
native code in VIctory's process. DLL lookup is absolute and dependency
search uses the DLL folder and System32.

## Adapter contract (Windows x64, ABI 1)

An independently developed adapter exports these C-callable symbols,
using the PC FidelityFX SDK's types and calling conventions:

- `uint32_t victoryStreamlineBridgeAbi(void)`: returns 1.
- `ffxCreateContext`, `ffxDestroyContext`, `ffxConfigure`, `ffxQuery`,
  `ffxDispatch`: PC FidelityFX-compatible exports, as used in
  `crates/victory-hle/src/modules/fidelityfx.rs`.

The adapter must support the context-free GetVersions query (type 4,
create type 0x10000), including the count-only and names queries on the
actual D3D12 device. It must report zero versions or an error if it cannot
serve the device, driver or SDK configuration. Return static NUL-terminated
names and respect supplied array capacity. A failed startup probe must leave
no pending GPU work, callbacks or background threads: VIctory can unload
the DLL. Successful provider code stays loaded for the process lifetime.

The title's create chain carries flags, max render/output sizes, device
and API version; the existing HLE substitutes the PC device descriptor.
The dispatch is the PC layout with native command list, native D3D12
textures (color, depth, motion vectors, exposure, reactive mask,
transparency mask and output), jitter, motion scale, render/output sizes,
sharpening, frame delta, pre-exposure, reset, camera near/far/FOV,
view-space scale and flags. Use the actual SDK headers and verify layouts
against the Rust compile-time assertions; do not reinterpret Streamline's
C++ ABI as this ABI. The HLE logs first-dispatch metadata for integration
diagnostics, not image data.

The adapter must implement the FidelityFX queries/configure operations the
title uses (including jitter phase/offset and memory/provider queries).
Do not return success without having produced the requested output.
Leave all textures in their declared states after recording; VIctory
restores its own bound heaps, pipeline and root signatures before its next
draw. The adapter must respect resources and context lifetimes until the
GPU has completed their last use, including work recorded but not yet
submitted. This needs a verified submission/retirement integration before
real DLSS evaluation is enabled.

## Next implementation milestone

- Direct NGX DLSS Super Resolution is a possible narrower prototype:
  [official SDK](https://github.com/NVIDIA/DLSS) and
  [NGX guide](https://docs.nvidia.com/ngx/programming-guide/index.html).
- Streamline is the longer-term framework:
  [DLSS guide](https://github.com/NVIDIA-RTX/Streamline/blob/main/docs/ProgrammingGuideDLSS.md),
  [manual integration](https://github.com/NVIDIA-RTX/Streamline/blob/main/docs/ProgrammingGuideManualHooking.md).
  Its camera matrices, frame tokens, resource tags, device registration
  and shutdown/retirement need integration. Camera matrices are not in
  the current FidelityFX dispatch; do not fabricate them.
- Validate inverted depth, motion vector scale/sign, exposure, jitter,
  texture formats and output dimensions on a moving scene. The upscaler
  consumes the game's existing render dimensions; it cannot independently
  lower internal rendering resolution.
- Frame generation and Reflex require presentation/latency integration
  and remain out of scope.

No NVIDIA SDK files, headers or binaries are vendored. The project's GPL
license and the SDK license/redistribution terms need review before shipping
a real adapter. See [DLSS license](https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt)
and the Streamline repository's license and third-party notices.

## Validation

Run `cargo test -p victory-core streamline_path` and
`cargo test -p victory-hle fidelityfx` (Windows for HLE), and the
existing FidelityFX integration fixture with a supported PC DLL:
`cargo test -p victory-hle --test fidelityfx`.
Test an unset path, an empty folder, a wrong ABI, a valid adapter reporting
zero versions, and a supported adapter. With the first four, verify
FidelityFX fallback if configured. No NVIDIA adapter or hardware test
has been run by this change.
