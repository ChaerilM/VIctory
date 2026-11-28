# Compact partial texture uploads

The existing watched-memory texture path identifies affected subresources
and detiles only those. Previously its upload allocation still reserved
the full texture's copyable footprint size. A partial upload now packs
only selected subresources at 512-byte placement alignment, retaining the
driver's row pitch, dimensions, format and byte extent. Each copy targets
its original subresource index. Full uploads retain the original offsets.

This reduces transient upload/staging memory use for partial updates.
It does not change texture dimensions, descriptor mip ranges, GPU texture
allocation, source-memory scratch allocation or texture eviction policy.
No new environment setting is required. Savings depend on which levels
change and on the staging pool's chunk allocation granularity.

True on-demand mip residency in VItoriaBr uses reserved resources,
queue-ordered tile mappings, packed mip tails and descriptor remakes.
Those changes require separate resource lifetime and descriptor work;
this change deliberately does not claim to implement them.

## Validation

Run on Windows:

```powershell
cargo test -p victory-gpu --lib texture::tests -- --nocapture
```

The new pure footprint tests cover sparse mip selection, placement
alignment, preserved full-chain layout, depth pitch and the last
unpadded row. Run existing texture upload tests and D3D12 validation,
including streamed mips, arrays and block-compressed textures. Compare
staging allocation high-water marks under the same workload.
Compilation, GPU execution and performance have not been verified.

This change overlaps the experimental GPU detile PR (#9) in the Upload
representation; reconcile the two before merging both. The GPU path
must continue to use its own complete-output footprint layout.
