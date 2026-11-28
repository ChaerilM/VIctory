# NVIDIA integration: descriptor allocator

First integration step based on [VIctory-NVIDIA #444](https://github.com/trustno1-coder/VIctory-NVIDIA/pull/444).
The source uses a packed bitset for dynamic descriptor slots.
This adaptation keeps this branch's exact first-fit cursor behavior,
including runs crossing the cursor but not the end of the heap.
A free-count check skips fruitless searches in exhausted heaps.
The mutex, descriptor indices, retirement and ownership remain the same.
No changes to shader-visible heap size, descriptor remapping, shader
translation or resource lifetime. No new environment switch.

The bitset requires about one sixteenth of the original byte-per-slot
bookkeeping (rounded to complete words). CPU time savings depend on
occupancy and contention; no measured speedup is claimed.

Already present on main: bit-walking live sets (source #651), concurrent
cache publication fallback (#632), runtime cache write failure fallback
(the runtime portion of #636), and null CBV recording (#646). Explicit
precompilation error reporting remains a separate candidate.

Validation: run `cargo test -p victory-gpu --lib dynamic_bitset` and
the existing dynamic descriptor heap tests on Windows. New tests compare
allocation and release sequences against a slot-by-slot reference, cover
padding, cross-word runs and duplicate releases. Compilation and GPU
tests have not been run. The integration branch is a draft starting
point; it does not yet include the larger memory, relay or upscaler
infrastructure. Source PR #444 remains open; its results do not validate
this adaptation.
