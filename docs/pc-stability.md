# PC stability investigation

This branch is experimental. Passing unit tests does not establish that the
title is playable, renders correctly, or survives world loading.

## Changes retained so far

1. Shader heap-handle cache keys include both resource-property words.
   Component type/count and buffer stride must distinguish otherwise
   identical descriptors. Imported from NicolaiVdS's `ae7e0be`, with
   authorship preserved. The synthetic size-query/atomic regression fails
   under the original cache behavior and passes with the complete key.
2. Shader-visible heaps resynchronize CPU descriptor writes before queue
   submission. Imported from Slaktarn700's `6f732b6`, with authorship
   preserved. A new synthetic regression copies and clears a descriptor
   directly in write-watched memory, without invoking a descriptor-copy
   API, and checks repeated host synchronization. A follow-up moves
   synchronization after deferred command-list copies and under the host
   submission lock; the regression also covers a deferred descriptor copy.
3. The fallback presentation window fits its client and caption inside
   the primary desktop work area. The swap-chain/render-target size is
   unchanged. Aspect-ratio tests cover small desktops and portrait and
   square content.

## Local checks

From the repository root, using its pinned Rust toolchain:

```powershell
cargo test --workspace --locked
cargo test -p victory-gpu --features diagnostics --locked -- --test-threads=1
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo xtask policy
cargo build --profile diag -p victory --features diagnostics --locked
```

Use `victory shader-info <files> --hash <runtime-hash> --disasm` to locate
one failing shader's resource metadata. Keep the resulting game-derived
listing outside the repository.

Diagnostic GPU tests can use WARP by setting `VICTORY_D3D12_WARP=1` in
that shell before running the diagnostic test command. WARP's supported
wave sizes can differ from the title's requirements; it is a diagnostic
adapter, not a complete compatibility fallback.

## Live-test discipline

- Keep supplied game files untouched and outside the repository. Use
  separate data, cache, capture, and log directories for each experiment.
- Record source revision, renderer, console model, validation options,
  start/end times, exit code, and the last observed stage.
- Let the tester advance first-boot and menu screens. An account/player
  prompt awaiting controller A is not itself a rendering failure.
- Compare visible rendering and input/audio behavior as well as crash
  logs. FPS and process lifetime alone are insufficient.
- Recheck game-file integrity after runs. Do not commit game files,
  captured frames, derived shaders, or memory dumps.
- Revise or revert a demonstrated regression. Keep independent changes
  in separate commits and compare equivalent run configurations.

## Candidate: scalar and vector buffer consistency

The isolated `codex/heap-buffer-consistency` branch addresses one shader
annotation inconsistency. An unreflected descriptor loaded directly from
ResourceHeap could receive a CBV handle for scalar loads and an SRV handle
for vector loads. A single descriptor cannot match both annotations.

The candidate preserves explicit used-resource metadata. For one direct
heap descriptor load used by vector buffer operations, scalar loads use
the same buffer class; a store or atomic makes its reads UAV accesses.
Known UAV scalar loads also use the raw UAV path. Scalar-only descriptors,
aliases from different load sites, formats, and structured strides remain
unresolved. This inference cannot establish the runtime type of every
unreflected descriptor and is not a complete descriptor-typing solution.

A synthetic regression reproduces mixed CBV/SRV annotations without the
candidate, checks explicit CBV precedence and UAV consistency, and passes
with it. A separate synthetic GPU test validates the DXIL and reads back
two distinct values loaded through scalar and vector accesses on WARP.
Live validation is pending.

## Remaining work

The live investigation still encounters device removal. GPU validation
also reports descriptor dimension mismatches and compute kernels requiring
unsupported wave sizes. Other shaders remain untranslated. None of those
gaps is resolved by the changes above. Do not remove required wave-size
metadata, drop additional work, or substitute undefined view types merely
to claim a stable run.

The architecture and all source modules have been inventoried; detailed
review is proceeding through the failing subsystems. An exhaustive review
and a complete PC port have not been completed.
