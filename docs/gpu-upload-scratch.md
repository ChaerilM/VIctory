# Experimental GPU upload scratch (NVIDIA / ReBAR)

This is a narrow adaptation of the GPU upload heap approach used by
[VItoriaBr](https://github.com/uiliannn/VItoriaBr/blob/main/crates/victory-gpu/src/shadow.rs)
and described in
[NVIDIA's DirectX 12 GPU upload heap guide](https://developer.nvidia.com/blog/optimizing-dx12-resource-uploads-to-the-gpu-using-gpu-upload-heaps/).

Set `VICTORY_GPU_UPLOAD_SCRATCH_MB` to a logical live allocation cap in MiB
(0 by default, up to 1024). Suggested initial experiment: 64 MiB, then
128 MiB or 256 MiB on cards with room. No profile enables it automatically.

```powershell
$env:VICTORY_GPU_UPLOAD_SCRATCH_MB = '64'
.\target\release\victory.exe run 'D:\game\title.exe' --execute
```

The process must report GPU upload heap support through D3D12 OPTIONS16
(usually requires ReBAR on discrete GPUs). Only newly allocated reusable
command-list scratch chunks (up to 4 MiB each) can use this heap. These
are CPU-written, GPU-read constant-buffer alignment copies and rectangle
vertices. They stay in GENERIC_READ, with no UAV flag. The normal upload
heap remains the fallback if the cap is reached, the driver's current
local budget would exceed 80%, the feature is unsupported, or allocation
or mapping fails. The budget counts the chunk's logical bytes until
destruction, including the shared spare pool. Driver allocation overhead
and paging are not covered by this cap; the budget check is advisory.
Oversized dedicated scratch, other upload resources and all writable
title-memory shadows are unchanged.

This is NOT VItoriaBr's `VICTORY_CARD_BUFFERS_MB` migration: its writable
buffers have a second CPU-side buffer and GPU-side compare/readback.
The present translator's title shadow is a single CPU-readable/writable
mapping. Moving it wholesale would risk stale data or expensive CPU VRAM
reads. That work needs its own coherency implementation and tests.

## Test plan

- Run `cargo test -p victory-gpu gpu_upload` on Windows. The hardware
  fixture skips when the device reports no support.
- Test an identical scene with 0, 64, 128 and 256 MiB.
- Compare median and p95 frame times, memory budget, D3D12 validation and
  image correctness. Use a warmed pipeline cache and the same settings.
- Inspect debug logs for allocated chunks; the supported flag alone
  does not prove an allocation was made. Clear the variable to disable.

No hardware benchmark has been run for this adaptation.
