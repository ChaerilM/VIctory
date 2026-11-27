# Experimental GPU texture detiling

This is a bounded adaptation of [fxceless/victory-fork commit 3461f52](https://github.com/fxceless/victory-fork/commit/3461f52faab6eff2f8526b1eb36037973674414f).
It uses the existing layout's pattern tables, a native D3D12 compute pass,
and the dedicated submission-time upload list. No proprietary shaders
or binaries are included.

Enable explicitly in PowerShell:

```powershell
$env:VICTORY_GPU_DETILE = '1'
```

Unset it or set it to 0 for the original CPU path. Only fully initialized
snapshots of complete tiled, single-sample textures at least 256 KiB are
eligible. Existing partial watched-memory refreshes, zero initialization,
linear textures and unsupported layouts use the CPU path. The original
change detection and copy ordering remain. This does not implement
on-demand mip residency or change texture resolution.

Additional committed workspace (raw input and output, charged in 64 KiB
units) is limited to 64 MiB per serialized submission. The local memory
budget is checked before allocation; if unavailable or above 80%, use
the CPU path. The check is advisory, not a hard VRAM cap. GPU allocations
are held by the upload list until reset after the preceding submission;
allocation and PSO creation failure fall back before recording. GPU
execution errors do not have a mid-submission fallback.

The GPU reads raw tiled bytes from upload memory. This can cost more
PCIe traffic than CPU detiling, especially for sparse changes, and adds
compute work on the card. The thresholds are conservative provisional
choices, not measured optimal values. No FPS gain is claimed.

## Validation

```powershell
cargo test -p victory-formats detile_params_address
cargo test -p victory-gpu --lib gpudetile -- --nocapture
```

The format test compares addressing against the existing CPU layout; the
Windows WARP fixture compares each actual row against CPU detiling,
including small element widths, odd rows, block-compressed formats,
arrays and 3D layouts. If WARP is not available the GPU test skips.
Run with D3D12 validation, compare identical scenes with the switch off
and on, including GPU upload-list execution, repeated submissions and
memory pressure. The fixture is not a full integration test. Neither
compilation nor the tests have been run for this port.
