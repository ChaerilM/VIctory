# Experimental parallel buffer refresh

Inspired by VItoriaBr's `VICTORY_WIDE_REFRESH` implementation
(`crates/victory-gpu/src/host.rs` and `shadow.rs` in
[uiliannn/VItoriaBr](https://github.com/uiliannn/VItoriaBr)).
This is a narrower adaptation to the current synchronous submission path.

Enable before starting the application:

```powershell
$env:VICTORY_WIDE_REFRESH = '1'
```

Unset or use 0 for the serial path. Only consecutive refresh jobs for
dedicated buffers with no pending shader-write declaration are candidates;
each request must be at least one 64 KiB title page. A group must contain
at least four jobs totaling at least 1 MiB, with disjoint virtual page
ranges. Shared view arenas, small or overlapping groups and pending
writes stay serial. The existing watch layer synchronizes physical aliases.

The same scoped worker helper as texture uploads is used (up to four,
at most one quarter of the available CPUs). Every job joins before a
later deferred step or GPU submission. Original merged ranges, write
watch, copy-back, deduplication and title-write flush boundaries remain.
No additional GPU allocation is introduced. Preparation timing records
wall time for parallel groups; individual diagnostic ranges and watch
counters still sum their own work and can exceed wall time.

This does not place title buffers in VRAM or make GPU submission
asynchronous. The fork's card-buffer implementation needs GPU snapshots,
dirty-range upload, compare/readback, merge and retirement support that
is not present here. Replacing shared UAV-capable mappings with upload
heaps alone is not equivalent. Those changes remain separate work.

## Validation

```powershell
cargo test -p victory-gpu --lib wide_refresh -- --nocapture
cargo test -p victory-gpu --lib a_copy_is_refreshed_once_per_submission
```

Compare identical workloads with the switch off/on, including watched
and untracked memory, aliases, mapping changes, mixed GPU/CPU writes,
and preparation time. The fork itself notes an earlier wide-refresh
experiment regressed copy time (52 to 68 ms); contention and worker
startup may outweigh the benefit. Thresholds are provisional. No
compilation, Windows tests or benchmark have been performed on this port.
