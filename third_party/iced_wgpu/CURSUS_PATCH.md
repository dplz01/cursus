# iced_wgpu 0.14.0 — Cursus patch

Vendored from crates.io `iced_wgpu 0.14.0` (MIT, © Héctor Ramón and iced
contributors) and wired in with `[patch.crates-io]` in the workspace
`Cargo.toml`. Two changes:

- `window/compositor.rs` `configure_surface`: the surface also gets
  `TEXTURE_BINDING` when the adapter supports it, and
  `iced_wgpu::BACKDROP_READABLE` (`lib.rs`) records whether it did.

That lets `linux/src/glass.rs` sample the frame drawn so far and blur it
under the floating composer and destinations bar (backdrop blur). Without
it the glass falls back to a dense frosted fill.

To update iced: copy the new `iced_wgpu` release here and re-apply the
change above (search for "Cursus patch").

- `window/compositor.rs` `Compositor::request`: with all backends allowed
  (the default), Vulkan / Metal / DX12 are tried first and GL only when
  they find no hardware adapter. Bringing GL up beside Vulkan loaded
  Mesa's GL stack (gallium + LLVM, ~25 MB resident) for nothing.
  `WGPU_BACKEND` still picks backends explicitly.
