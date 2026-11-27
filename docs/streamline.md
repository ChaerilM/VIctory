# Experimental NVIDIA DLSS Super Resolution (direct NGX)

This optional, locally built provider translates the console FidelityFX
upscaling API to native NVIDIA NGX DLSS Super Resolution. It keeps
`streamline_dir` and the adapter filename from phase 1 for compatibility;
it does not yet call Streamline. No SDK files or native binaries are
committed, and no SDK dependency is added to Cargo.

## Build locally

Obtain the [official DLSS SDK](https://github.com/NVIDIA/DLSS) outside the
repo and review its [license](https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt).
From an x64 Visual Studio Developer PowerShell:

```powershell
.\scripts\build-ngx.ps1 -SdkDir D:\SDKs\DLSS -OutputDir D:\victory-data\streamline
cargo build --release -p victory
```

The script uses the SDK's Windows x64 headers and MD-runtime library
and copies its release feature DLL to the local output directory.
Do not commit or distribute the resulting DLLs without reviewing SDK
and GPL licensing compatibility and any applicable permissions.
The script checks the stated folder layout; it is untested on Windows.

Configure your private `victory.toml`:

```toml
[paths]
fidelityfx_dir = 'D:\victory-data\fidelityfx'
streamline_dir = 'D:\victory-data\streamline'
```

Set the environment variables for your own development project and local
SDK log/data storage outside the repo (use a project UUID you control,
not one belonging to another application):

```powershell
$env:VICTORY_NGX_PROJECT_ID = '<your-project-uuid>'
$env:VICTORY_NGX_DATA_DIR = 'D:\victory-data\ngx-data'
```

A UUID may be generated once for a local project with
`[guid]::NewGuid().ToString()`; keep it stable. A valid ID does not
guarantee NVIDIA service approval or feature availability. The SDK may
perform its own updates as specified in its license.

`VICTORY_STREAMLINE_DIR` overrides the TOML path. Use trusted local DLLs.
At startup the HLE tries an NVIDIA provider on the actual NVIDIA D3D12
device, then the configured PC FidelityFX provider, then the original
console API. If the provider is selected but later fails to create or
evaluate a feature, restart without its path; no mid-context fallback is
implemented. The raw Streamline SDK DLLs alone are not an adapter.

## Implementation

- ABI 2 requires five PC FidelityFX-compatible calls, the existing version
  marker and explicit retain/release exports. ABI 1 is intentionally
  rejected so recorded GPU work cannot outlive its adapter context.
- The context owns NGX parameters and the feature handle. Initialization
  uses an independent direct list and waits for a fence. HLE retains
  context references on recorded host lists, including on dispatch error,
  until reset or destruction after queued use.
- Feature creation queries NGX capability and optimal-settings ranges
  for the actual dimensions. It does not force a lower render size.
- Dispatch maps native color/depth/motion/output, exposure, jitter,
  motion-vector scale, frame time and reset; validates dimensions and
  states. Combined SRV state is converted and restored around evaluation.
- HDR, inverted depth, auto exposure and motion-vector flags are mapped.
  Unknown flags, unsupported queries and unsupported configurations are
  rejected rather than pretending they ran.
- FSR quality-ratio and Halton jitter queries keep familiar semantics.
  Memory query uses SDK statistics (process total, not per-context).
  FSR reactive and composition masks, sharpening, frame generation and
  Reflex are not implemented. No fabricated camera matrices.

## Validation

The adapter has not been compiled or run on Windows. It remains a draft
prototype. Run the existing synthetic GPU readback fixture with the
adapter explicitly selected, which refuses silent fallback:

```powershell
$env:VICTORY_TEST_NGX = '1'
$env:VICTORY_STREAMLINE_DIR = 'D:\victory-data\streamline'
cargo test -p victory-hle --test fidelityfx -- --nocapture
```

Set the NGX project and data variables above first. The fixture records
two upscales, releases title ownership before submission for NGX, checks
the output and D3D12 validation messages. Run again with the flag removed
and a supported PC FidelityFX SDK to verify the baseline. A failed fixture
is a blocker, not evidence of compatibility.

Before enabling for normal use, test moving scenes, jitter sign,
motion-vector scale and history reset, render-size changes, queue/list
ordering and repeat submissions, resource format support and context
retirement. NGX internal synchronization and parameter lifetime across
multiple recorded lists remain unverified. Compare actual frame times
with the same rendering dimensions. No speedup is promised.

Streamline and additional features remain future work. See the
[NGX programming guide](https://docs.nvidia.com/ngx/programming-guide/index.html)
and [Streamline SDK](https://github.com/NVIDIA-RTX/Streamline).
