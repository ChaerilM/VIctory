# Indirect clamp fast path and counted executions

Inspired by the count-aware clamp in
[uiliannn/VItoriaBr](https://github.com/uiliannn/VItoriaBr)
(`crates/victory-gpu/src/indirect.rs`). Adapted to the existing
ordered single-group clamp rather than adopting multi-group
charging or gather-ahead.

The GPU clamp reads the optional count buffer at its original command
position. It limits the records to min(max, count), so inactive records
do not consume budget or require source reads. ExecuteIndirect retains
its original hardware count buffer. This assumes the same queue-ordered
count value remains valid until execution, as in the existing path.
The single-record path charges the same budget and clamps the same
fields, using lane zero without group synchronization. The multi-record
path retains ordered charging and adds a rendezvous between tiles to
avoid overwriting shared costs before their prior consumers finish.

Invalid kinds, misaligned addresses, undersized strides and lengths
that overflow 32-bit byte addressing are rejected before allocations
or shader recording. Existing count limits, per-list work budget,
source buffer refresh, resource barriers and argument buffer ownership
remain unchanged. No setting is needed. This is not gather-ahead,
and does not move clamping before preceding argument producers.

## Validation

```powershell
cargo test -p victory-gpu --lib indirect::tests -- --nocapture
```

Added layout checks and synthetic GPU tests for the count-limited
budget and single-record bounds. Existing ordered-budget and resource
state tests remain. Run the tests on Windows with the debug layer,
then benchmark identical indirect-heavy workloads. Neither compilation
nor GPU execution has been performed for this port. A single-group
multi-record clamp remains; final performance is unmeasured.
