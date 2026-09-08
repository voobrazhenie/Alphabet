# Working notes for the native build

`README.md` says what the app is and how to use it. This file is the other half:
what a session picking this up needs to know before changing anything, and the
things that cost time to find out the first time.

---

## Where it stands

Everything here was written and checked in a Linux container **with no GPU**. The
shader is proved correct (see below) and the Windows build compiles, links and
passes its tests in CI — but nobody has watched the app draw a frame on real
hardware. The first person to run it is the user, on a Lenovo Legion with a
GeForce 4070 and Windows 11.

So: **treat the first report from that machine as the first real test.** Runtime
faults are more likely than rendering faults. The likely suspects, in order:

1. **Switching to OpenGL and back.** Windows lets a window's pixel format be set
   only once, so a window that has carried a WGL context may refuse the next one.
   `App::build_gfx` already retries once on a brand-new window before falling back
   to another API — that path has never actually run.
2. **Exclusive full screen.** `Fullscreen::Exclusive` picks the monitor's largest,
   highest-refresh mode. Borderless is the default and the safe one.
3. **Uncapped presentation.** `PresentMode::Immediate` is asked for and quietly
   downgraded when a driver does not offer it; the console greys out what is not
   available.
4. **DPI.** The app never multiplies by a scale factor — winit's physical size is
   already device pixels — so a display at 125% or 150% should be right, but has
   not been seen.

## Checking a change

Three checks, cheapest first. All of them run without a GPU.

```
cargo test                     # 16 tests: shaders, uniforms, benchmark, camera, settings
NODE_PATH=/opt/node22/lib/node_modules node tools/parity.mjs
cargo check --target x86_64-pc-windows-msvc
```

- **`cargo test`** is the one that matters most. `tests/shaders.rs` parses every
  object's WGSL with naga and then runs the **SPIR-V, HLSL and GLSL** backends over
  it — the same three translations wgpu performs for Vulkan, DirectX 12 and
  OpenGL — so a shader that would only fail on one backend fails here instead. It
  also checks the uniform block in the shader against `gfx::Uniforms` by size *and*
  field order; that pairing is the difference between a picture and garbage.
- **`tools/parity.mjs`** is the proof the port is faithful. It emits the ported
  shader as GLSL the way wgpu's GL backend would, then renders it and the original
  `#fs` shader out of `../neurons.html` in headless Chromium (SwiftShader) with
  identical uniforms and compares the two images. Six cases across the three
  objects, currently **exactly** identical. Run it whenever either shader changes.
  Playwright lives in the container's global modules, hence `NODE_PATH` and the
  `createRequire` at the top of the script — an ESM `import` cannot see `NODE_PATH`.
- **`cargo check --target x86_64-pc-windows-msvc`** type-checks the Windows tree,
  including the DirectX 12 backend, without needing a linker (`rustup target add`
  it first). For a real link, `apt-get install mingw-w64` and
  `cargo build --target x86_64-pc-windows-gnu` — that one links the whole thing.
- CI (`.github/workflows/pc-windows.yml`) is the authority: `windows-latest`, tests
  then release build, and the `.exe` uploaded as an artifact. Documentation-only
  changes under `pc/` skip it.

## Things that will silently break the picture

Each of these was a deliberate decision. Undo one by accident and the app still
runs — it just looks wrong, or dies on one backend only.

- **The y flip in `fsMain`.** WGSL's `@builtin(position)` counts down from the top;
  GLSL's `gl_FragCoord` counts up from the bottom. The entry point flips it so the
  ported shader means the same thing as the page's. wgpu's GL backend sets naga's
  `ADJUST_COORDINATE_SPACE`, so `gl_FragCoord` lands where `@builtin(position)`
  does and the same flip is right on all three backends. It is also why
  `parity.mjs` compares row `y` against row `H-1-y`.
- **The surface format must not be sRGB.** The shader applies its own gamma; an
  sRGB surface applies it a second time and everything washes out. `Gfx::new`
  picks the first non-sRGB format the surface offers.
- **The offscreen target uses the surface's format**, not a fixed one. That is what
  lets the same pipeline draw either into the target or straight to the screen —
  the `direct` path that skips the post pass when nothing needs resolving.
- **The march loop must stay impossible to unroll.** Its trip count comes from
  `U.steps` through a `clamp`, so no compiler knows it. Put a constant bound back
  and DirectX's older FXC compiler may try to lay 300+ iterations out flat — which
  is exactly the failure the web page carries a "retry with a shorter march"
  workaround for.
- **egui's texture deltas must be applied on every frame, even a skipped one.**
  egui hands over each change once and never repeats it; drop one and the console
  loses its font atlas for the rest of the session. `Gfx::render` uploads before
  acquiring the swapchain image and frees after submitting.
- **A new device needs a new egui `Context`.** The renderer lives inside `Gfx`, so
  switching backend throws it away — and the old context would never re-send the
  atlas to the new one. `App::adopt` rebuilds both together.
- **The benchmark's determinism** is a contract, written down in
  `../REQUIREMENTS.md` §11: four fixed shots, 150 frames each, clocks stepped by
  exactly 1/60 s per frame from fixed seeds, march budget frozen, adaptive
  controllers off, console not drawn. Change any of it and native numbers stop
  comparing with browser numbers — and with older native ones. `tests/behaviour.rs`
  runs the same benchmark at 250 fps and at 36 fps and demands identical poses.

## Versions, and how to survive a bump

Pinned in `Cargo.toml`: **wgpu 30.0, winit 0.30.13, egui / egui-wgpu / egui-winit
0.36**. These have to move together — `egui-wgpu 0.36` requires `wgpu ^30`, and
egui 0.36 needs rustc 1.95 or newer (the container shipped 1.94; `rustup update
stable` fixed it).

Both crates change their API freely between versions, and the changes are not
guessable. **Read the crate source instead of guessing** —
`/root/.cargo/registry/src/*/wgpu-30.0.1/src/api/` and the matching `egui-0.36.1`
— then compile early and let the errors finish the job. For the pinned versions,
the traps already paid for:

| | |
| --- | --- |
| `Instance::new` | takes the descriptor by value; `InstanceDescriptor::new_without_display_handle()` |
| `RequestAdapterOptions` | carries `apply_limit_buckets` |
| `SurfaceConfiguration` | carries `color_space` (`SurfaceColorSpace::Auto`) |
| `PipelineLayoutDescriptor` | `&[Option<&BindGroupLayout>]`, and `immediate_size` where push constants used to be |
| render pass / pipeline | both want `multiview_mask`; colour attachments want `depth_slice` |
| `get_current_texture()` | returns the `CurrentSurfaceTexture` **enum**, not a `Result` |
| presenting | `queue.present(frame)`, not `frame.present()` |
| error scopes | `push_error_scope` returns a guard; `guard.pop()` is the future |
| egui | `Context::run_ui` (not `run`), `egui_wants_keyboard_input`, `all_styles_mut`, `content_rect`, `Button::selectable` (there is no `SelectableLabel`), and `TexturesDelta.set` holds a `SmallVec` of deltas per id |

`rustfmt.toml` is tuned to how the code is already written, so `cargo fmt` is a
no-op rather than a forty-file diff. Keep it that way.

## Deliberately not done

- **No DirectX 11.** wgpu has no DX11 backend any more. DX12, Vulkan and OpenGL
  are the comparison.
- **DXC is not statically linked.** `wgpu`'s `static-dxc` feature would bundle
  Microsoft's newer shader compiler (it only affects the MSVC target), but it
  downloads a prebuilt binary at build time and the old compiler handles this
  shader — the page already proves that through ANGLE. If DirectX 12 ever refuses
  to build an object on real hardware, that feature is the first thing to try.
- **No network, no cloud sync.** The page's Firestore path is not ported and
  should not be. `settings.json` and `templates.json` are their own format and are
  *not* interchangeable with the page's saved settings.
- **The GL backend passes no display handle** to `InstanceDescriptor`, which is
  fine on Windows and would need `new_with_display_handle` to work on Wayland.
- **No installer, no code signing.** SmartScreen warns once on an unsigned binary.

## Layout reminder

`src/state.rs` mirrors the page's `state` object field for field — start there when
adding a control, then `src/ui.rs` for the console row. `src/app.rs` holds the frame
loop, the keys and the adaptive controllers; `src/gfx.rs` is the only file that
knows about wgpu. The three-line `//#ifdef` preprocessor in `src/shaderpp.rs` is
what keeps one WGSL file serving three objects, one pipeline each.

**The web page is not ours to touch.** `neurons.html` and `REQUIREMENTS.md` describe
the browser build; this folder follows them, and a behaviour change that belongs to
both is made on the page first.
