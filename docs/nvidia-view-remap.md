# Experimental capped view heap remapping

Adapted from [VIctory-NVIDIA PR #638](https://github.com/trustno1-coder/VIctory-NVIDIA/pull/638).
Enable before starting VIctory (including cache warm-up):

```powershell
$env:VICTORY_VIEW_REMAP = '1'
```

The setting is process-wide and off by default. It uses a physical
1,000,000-entry shader-visible view heap, preserves the first 65,536
logical title slots where possible, pools subsequent title slots and
reserves 262,144 contiguous dynamic slots plus three null descriptors.
The logical title+dynamic range must fit below the existing 2^21 limit.
Sampler heaps are unchanged. The experimental mode intentionally caps
even larger-capacity adapters when enabled; use it only for testing.

The second half of the stride table maps logical indices to physical
places. Only createHandleFromHeap reads that mapping; stride, mark,
address and read-feedback indices stay logical. Raw null SRV/UAV
indices are mapped to separate physical nulls. Host-created memory
view pairs use the dynamic region. The shader cache fingerprint and
pipeline warm-up tag include the process setting.

Pooled places are published after descriptor creation and retired
without overwriting the physical descriptor. They can be reused only
after a subsequent serialized GPU-complete submission. This adapts the
source's FIFO heuristic to the current synchronous submission model.
Pool exhaustion leaves a slot null and retries on a later mirror;
no write is sent to a shared sink/null descriptor. Existing null
descriptor kind limitations and concurrent descriptor update semantics
remain; the change does not guarantee correctness for all resource
kinds or eliminate all GPU faults.

## Validation

Run `cargo test -p victory-gpu --lib remap_places`, existing descriptor
tests with the setting off and on, and shader/PSO validation with an
NVIDIA adapter. Verify the mapping for high indices, dynamic tables,
title memory views, texture eviction, pool exhaustion and descriptor
updates racing submission. The pure allocation test covers disjoint
ranges, nulls, exhaustion and delayed reuse; it is not an end-to-end
GPU fixture. The new mode has not been compiled or tested on Windows.
Do not enable by default or combine with asynchronous submissions
until the retirement model is revisited and validated.
