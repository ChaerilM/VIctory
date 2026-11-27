# Shader arithmetic and wave compatibility

Adapts the legacy-multiply optimization from
[fxceless/victory-fork #1386](https://github.com/fxceless/victory-fork/pull/1386)
to the existing exponent-bit semantics. It preserves signed-zero and
denormal flushing even when the other operand is NaN or infinity.
Identical operands still require one exponent check; known nonzero-exponent
constant operands do not need a dynamic check. A known exponent-zero
operand folds to +0 without emitting the product. The legacy multiply-add
path uses the same product lowering and retains its existing addition.

The arithmetic optimization is vendor-independent. Reduced emitted
operations may benefit NVIDIA and other drivers; final machine instruction
count and frame-time effect are not measured.

The ordinary `victory translate --list` report counts emitted compute
programs with recognized fixed Wave32 and Wave64 requirements and marks
the latter in its list. Keep input and reports local. The parser reads
only the current emitted PSV0 runtime-info layout. Unknown layouts and
no fixed requirement share a bucket; neither means guaranteed portability.

Runtime compute pipeline creation checks the selected D3D12 device's
OPTIONS1 wave-lane range. A recognized fixed size outside that range is
refused before root signature/PSO creation, with a distinct diagnostic.
Unknown capabilities still use the previous driver path. A supported
range is not proof of shader compatibility. No wave size is changed.
This avoids futile compilation work but does not make unsupported
shaders execute.

## Verification

- `cargo test -p victory-shader legacy_mul`
- `cargo test -p victory-shader fixed_compute_wave_size`
- On Windows, run the synthetic legacy multiply readback tests on
  supported hardware and WARP.
- Rebuild the shader cache after changing the translator (source hash
  already invalidates old translated entries). Compare fixed-wave
  counts, refused pipelines, identical-scene frame times and visual
  correctness. Run `victory translate --list` on local inputs.
- Wave64-to-Wave32 conversion remains separate work: explicit lane
  indexing, numeric execution masks, cross-wave LDS exchange and
  convergence must be preserved. Do not globally force Wave32.

No GPU or compilation validation has been performed for this change.
