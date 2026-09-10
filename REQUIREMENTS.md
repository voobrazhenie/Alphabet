# Functional requirements

What the page does today, written down so it can be checked, changed on
purpose, and rebuilt somewhere else (a native app) without guessing.

`neurons.html` is one self-contained file: a ray-marched WebGL shader with three
objects and a control console. No build step, no dependencies. The same file is
published as `index.html` in `voobrazhenie/liquidchrome`; that repository carries
only the built page, and this document lives here.

---

## 1. The render

- One full-screen fragment shader marches a signed distance field, one ray per
  pixel. There is no mesh and no fallback image: without WebGL the page shows a
  short explanation instead.
- Two passes. The scene is marched into an offscreen target, then a post pass
  puts it on screen — FXAA, or a plain resolve that lets the bilinear filter
  down-sample a 2x target.
- One shader program per object, compiled the first time that object is shown.
  Link status is checked: ANGLE translates GLSL to HLSL at link time, so a
  shader that compiles can still fail. A failure retries once with a shorter
  march before the object is marked unavailable.

## 2. Objects

Exactly one is shown at a time.

| Object | What it is |
| --- | --- |
| **Brain** | A lattice of somata and neurites inside a brain-shaped shell. Cell density is adjustable. |
| **Neuron** | One cell: soma, five dendrites with a fork on each, and an axon, built from Bézier tubes. Line width is adjustable. |
| **Chrome** (LiquidChrome) | A 20x20x0.5 slab, displaced by two octaves of Perlin into a landscape, intersected with an undeformed copy of itself, then twirled. Each of the five construction steps can be switched off on its own. A sixth switch, off by default, hangs a plain cube over the slab. |

Each object keeps its own two colours (structure and accent).

## 3. Camera

Two modes, switched in the console or with `shift+\``.

**Auto cam** flies one of three paths — Orbit, Fly-through, Drift. Dragging
steers relative to the path; the wheel or a pinch changes range.

**Fly** is free navigation, Blender-style:

- `W A S D` move, `Q` `E` down and up in world coordinates, `shift` sprints.
- Right-drag looks; pitch stops just short of the poles.
- The wheel (or a pinch) sets speed, 0.03 to 12 units per second.
- Taking over from auto cam starts exactly where the auto path left the camera.

`R` jumps to a new viewpoint. It always frames the object rather than empty
space, and stays close enough that the object overruns the frame.

**The framing never moves because of the interface.** Folding a console group,
hiding the panels or resizing the console must not shift the view by a pixel.

## 4. Saved viewpoints

Six slots. `shift`+`5 6 7 8 9 0` stores, the bare digit recalls. A slot holds
the whole camera state — fly position and heading, path mode, path clock,
steering and zoom — and recalling one restores the navigation mode it was saved
in. Slots are part of "Save as default".

## 5. Console

Four folding groups, plus an ungrouped footer.

- **Construction** — Object, Navigation, Flight path.
- **Object modifications** — the chrome build steps and their sliders, ray
  steps, surface precision, step relaxation, bounds and padding, cell density,
  domain warp, noise warp mode, warp strength and scale, line width, spike rate,
  colours, and the Flight / Morph / Impulses toggles.
- **Rendering** — Resolution, Antialiasing.
- **Shadows** — the Shadows switch, where the sun stands, and how its shadows
  fall. See §15.
- **Material** — the MatCap switch, which sphere it uses, and loading more of
  them. See §16.
- **Post** — what happens to the finished frame. See §17.
- Footer — Benchmark, Save as default / Reset, and two status lines.

Controls that belong to one object only are shown only for that object. Which
groups are open is remembered. The console is the one panel that scrolls, with
a styled vertical scrollbar and no horizontal one.

## 6. Rendering controls

| Control | Range | Default |
| --- | --- | --- |
| Resolution | Auto, Half, Native, FHD | Half |
| Antialiasing | Off, FXAA, SSAA x4 | FXAA |
| Ray steps | 48–320 | 320 desktop, 68 mobile |
| Surface precision | 1.0–12.0 | 12.0 |
| Step relaxation | 1.00–1.90 | 1.30 |
| Bounds | Sphere, Box, Auto | Auto |
| Bound padding | 0–0.60 | 0.05 |

- **Auto** hands the render scale to a frame-time controller. **Half** and
  **Native** pin it. **FHD** renders exactly 1920x1080 whatever size the window
  is, and asks the browser to match the window to it — which an ordinary tab
  refuses, so the page says which of the two happened. Chosen by hand FHD
  outranks the software-renderer clamp; restored from saved settings it does
  not.
- **Step relaxation** is over-relaxed sphere tracing: each step reaches past the
  safe radius by this factor, and a step whose sphere fails to touch the
  previous one is walked back. Above 1 it converges in fewer steps; too far and
  it spends them backing out.
- **Bounds** is the volume the march runs inside. Auto picks a fitted box for
  the chrome slab and a sphere for the rest.
- Distances are divided by an upper bound on the warp's Lipschitz constant, so
  a warped field is still a valid lower bound on the true distance and the
  march cannot step through a surface.

## 7. Deformation

Three deformations, each switched on its own: **Noise**, **Twist** (whirl about
Y), **Bend** (about Z). Strength 0–1.00, scale 0.30–4.00 per unit.

The noise deformation has two modes:

- **Space** bends the coordinate space the object sits in. Everything shears
  together — the melted, torn look. Every march step pays for it.
- **Object** leaves the space straight and pushes the object's own surface in
  and out along its normal by the same noise field. Real geometry: the
  silhouette moves and the normals follow. The height is a fraction of one noise
  lobe rather than a fixed length, so the slope stays constant across the scale
  slider, and the step penalty is only paid within reach of a lobe — 1.4x to
  1.7x faster than Space.

Twist and bend are space warps in both modes.

## 8. Animation

- **Flight** (`space`) stops the camera and the field together.
- **Morph** (`M`) animates the field.
- **Impulses** (`I`) are travelling lights, rate 0–11 Hz.

## 9. Saved settings

**Save as default** stores everything the console can set, the fly camera's
position and heading, the six viewpoint slots, and which groups are open.
**Reset** returns to the built-in settings.

Two stores behind one seam. The browser's own copy lands first so there is no
flash of the wrong settings; a Firestore document
(`claudecode-3bb06`, `settings/default`) overrides it only if it is newer, and
the same defaults then follow the page to any computer. The whole cloud path
fails soft: no network, no permission, no cloud, and the page carries on with
the local copy. The web API key is public by design; the Firestore rules are
what protect the data.

## 10. Statistics panel

Current frame rate, a true five-second mean (frames divided by the time they
took, not the mean of per-frame rates), the render size, and a graph of the last
five seconds.

The graph maps its x axis to time rather than to sample count, so it scrolls at
a steady rate instead of stretching as the frame rate changes. Each column keeps
the worst frame that landed in it. The scale is labelled down the left and steps
between 60, 120, 240 and 480 off the 90th percentile of what is drawn, with
hysteresis so it does not flip back and forth. A yellow line marks the
five-second mean and carries its own reading.

## 11. Benchmark

Runs from the console. The point is that the same settings give the same result
on the same machine, and that two machines do identical work — so the numbers
can be compared across machines, browsers, resolutions and modes.

### What makes it deterministic

- The camera follows a **fixed path**. The user's view, the saved slots and the
  navigation mode have no influence, and are restored untouched afterwards.
- The three animation clocks step by a **fixed 1/60 s per frame**, never by
  elapsed time, from fixed seeds. Frame *N* therefore shows the same geometry on
  every machine at every frame rate.
- The **march budget is frozen** at whatever the settings ask for when the run
  starts, and the adaptive resolution and step controllers are switched off for
  the duration.
- The frame-rate graph and the statistics panel do not update during a run, so
  they cost nothing and cost the same nothing everywhere.

Only wall-clock frame time is measured.

### The run

| | |
| --- | --- |
| Warm-up | 1.5 s holding the opening pose, discarded — shader caches and GPU clocks ramping |
| Shots | 4 |
| Frames per shot | 150 (600 measured frames) |
| Clock step | 1/60 s per frame |
| Clock seeds | camera 12.0, morph 40.0, impulse 6.0 |
| Time limit | 45 s, after which the run reports PARTIAL |

Typical total is 5–15 s. A machine below roughly 14 fps will not finish; that is
reported as PARTIAL and is explicitly not comparable with a complete run.

Every position is in units of the object's own radius (Brain 1.55, Neuron 1.15,
Chrome 1.10), and every shot passes above the object looking down at the middle.
`u` runs 0 to 1 across a shot.

| Shot | Path |
| --- | --- |
| Pass | line, `(-1.65, 0.55, 0.10)` to `(1.65, 0.55, -0.10)`, target `(0, 0, 0)` |
| Diagonal | line, `(-1.30, 0.95, -1.30)` to `(1.30, 0.35, 1.30)`, target `(0, 0.05, 0)` |
| Orbit | arc at radius 1.05, height 0.45, angle 0.60 to 2.70 rad, target `(0, 0, 0)` |
| Graze | line, `(0.15, 0.30, -1.75)` to `(-0.10, 0.22, 1.15)`, target `(0, 0.05, 0)` |

Roll is 0 throughout.

### The report

Average, median, 1% low (99th percentile frame time) and best 1% frame rates;
the frame count and elapsed time; the average for each shot; and the full
settings that produced them — object, resolution and actual render size, AA,
march budget, precision, relaxation, bounds, warp, the object's own parameters,
the GL renderer string, window size and device pixel ratio, and the user agent.
The whole report can be copied as text.

If the frame time sits steadily on a common refresh rate the report says so: the
display is the limit, not the render, and the resolution needs raising before
the number means anything.

### Reimplementing it elsewhere

A native build reproduces this run by following the table above: the same four
shots, 150 frames each, the same clock seeds and the same 1/60 s step, the march
budget held fixed, and no adaptive quality. Numbers are then comparable with the
browser's, given the same object and settings.

## 12. Keyboard

| Key | |
| --- | --- |
| `space` | Flight and morph together |
| `L` | Hand the left drag between the sun and the camera |
| `*` | Stand where the sun does, and come back |
| `M` | Morph |
| `I` | Impulses |
| `R` | New viewpoint |
| `1` `2` `3` `4` | Auto / Half / Native / FHD resolution |
| `A` | Cycle antialiasing |
| `F` | Full screen |
| `H` or `U` | Hide the panels |
| `` shift+` `` | Fly mode |
| `5`–`0` | Recall a viewpoint |
| `shift`+`5`–`0` | Store a viewpoint |
| `W A S D Q E`, `shift` | Fly (only while flying) |
| `Esc` | Close or stop the benchmark |

Keys are ignored while a text input has focus. `W A S D Q E` belong to fly mode
while it is on, so `A` does not also cycle antialiasing there.

## 13. Constraints and behaviour under stress

- Opening frames are capped hard: nothing has been measured yet, and a
  full-size first frame on a machine without acceleration looks exactly like a
  page that renders only its panels.
- A software renderer (SwiftShader, llvmpipe, Basic Render Driver) is detected
  and told to the user, with the march budget clamped.
- On a phone the march budget is lower and left to the frame-time controller.
- Everything degrades rather than fails: no WebGL, no network, no clipboard, no
  full-screen permission, no local storage — each is handled and said plainly.
- `prefers-reduced-motion` starts the page with the camera and field still.

## 14. Deliberately not done

- The page has no mesh export, no recording, and no MIDI or OSC input.
- No depth-aware upscaling; the resolution control is a straight render scale.
- The post pass still runs when antialiasing is off (a plain resolve).
- A pinned resolution can still be given up if the frame time becomes
  untenable (over 250 ms), and does not climb back on its own while pinned.

## 15. The sun and its shadows

On by default, and off is exact: with **Shadows** unset the key light is the
one that follows the view, no shadow ray is cast, and the frame is the one the
page drew before the sun existed, to the byte.

Switched on, a **sun** takes over as the key light. It stands still in the
world while the camera moves, which is the whole point — a light fixed to the
view throws its shadows behind the things that cast them, where nobody can see
them.

| Control | Range | Default | Applies to |
| --- | --- | --- | --- |
| **Shadows** | off / on | on | all |
| **Lit / unlit** | off / on | off | all — draws the shadow term itself, white where the sun reaches and black where it does not, with no material at all |
| **How** | Ray march / Sphere / Sun angle | Ray march | — |
| **Sun across** | 0-360 degrees | 52 | all — the sun is the key light in every mode |
| **Sun up** | -89 to 89 degrees | 34 | all |
| **Softness** | 0-1 | 0.25 | ray march |
| **Reach** | 0.5-6 units | 3.0 | ray march |
| **Shadow sphere** | any loaded sphere | Chrome | sphere |
| **Level** | 0-1 | 0.50 | all |
| **Contrast** | 0-1 | 0.00 | all |
| **Amount** | 0-1 | 0.85 | all |

### Three ways to arrive at one number

Whichever is chosen, the answer is a single value — 1 is full light, 0 is
black — and all three go through the same shaping and the same application, so
the controls mean the same thing in each and switching between them is a fair
comparison.

- **Ray march** — a second march from the lit surface toward the sun. The only
  one that can throw a shadow from one thing onto another, and the only one that
  costs a ray.
- **Sphere** — the brightness of a painted sphere, read the same way a matcap
  is (§16), in linear light. Whatever is painted on it lands on the object, for
  one texture fetch. It cannot know about anything else in the scene.
- **Sun angle** — how square-on the surface is to the sun, opened out across the
  whole range. No ray and no texture: a dot product. It cannot throw a shadow
  onto anything else either, but it turns with the sun and with **Contrast** up
  it draws a hard terminator wherever it is aimed.

For **Sphere** and **Sun angle** the console draws the result on a small sphere
beside the controls, after Level, Contrast and Amount — so what the sliders are
doing can be read straight off rather than hunted for on whichever part of the
object happens to point the right way.

Whichever source is chosen, the shadow also sits over a **MatCap** material
(§16) when both are on: the sphere is a picture, so it is multiplied in display
terms.

**Level** slides the edge between light and shadow; **Contrast** squeezes the
ramp around it until it is a step. That is where a soft grey shadow becomes a
hard black one with a boundary you can see. At **Contrast 0 the shaping is the
identity**, so the ray march is exactly what it was unless the slider is reached
for.

The ray march itself: a second march from the lit surface toward the sun, at most 64
steps and never further than **Reach**. It steps by the distance the field
guarantees is clear and judges a near miss by the distance the surface probably
is — the two are not the same number wherever a warp or the relief is on, and
stepping by the second one lands the ray inside the surface it set off from,
which reads as a hit and blackens faces the sun is plainly shining at. Being
inside is the sign of that second number and not a small value of it: calling a
hit at a hundredth of a unit is a cliff, and on a ridge lit edge-on by a low sun
neighbouring pixels fall either side of it and come back opposite. The ray also
starts clear of the surface by two things added together — more the lower the
sun sits on that face, since a ray leaving at a shallow angle stays inside the
surface's own roughness a long way out, and more again by however far the main
march may have overshot in landing, since a shadow ray born underground is black
before it starts. The closest that ray passes to anything, against how far it had
travelled, is how much of the sun it hides —
so the penumbra comes out of the same march rather than out of more rays.
**Softness** sets how wide the sun reads: 0 is a point and a hard edge, 1 is a
broad source. A ray that actually touches the surface is black whatever the
softness says, so a sharp shadow is properly black and not merely dark.

**Darkness** is how much a shadow is allowed to take. It scales the key light
and the highlight, and by the same amount the fill, the rim and the back
light — so at 1 a fully shadowed pixel is black, and at 0 nothing changes at
all. The glow is gathered along the view ray rather than at the surface, and is
left alone.

A surface facing away from the sun has no sun on it, which is what being in
shadow means — so it is in shadow, at once and without a ray. That is what makes
an object shadow itself rather than merely shade itself.

A shadow ray's step is the true distance divided by the warp's Lipschitz bound,
and that bound can be twenty-something. A ray that only ever takes those steps
never arrives: it spends its whole budget in the first tenth of a unit and gives
up — and giving up reads as **lit**, which is light leaking through the middle of
a shadow. Both ways of failing leak light, so the step also grows with how far
the ray has come, by 8% each time, which is what it takes for ninety-six of them
to cross the whole **Reach** whatever the warp is doing. Measured on a warped
scene, that took rays that ran out of budget from 45% to none.

## The sun's own view

`*` stands the camera where the sun is and looks back along it, and switches the
projection to **parallel** at the same time — the sun is a direction and not a
place, so a perspective eye put at one would show a view no shadow in this scene
was ever worked out from. From there nothing can be seen in shadow, which is the
point of it: it is what the sun sees. Dragging moves the sun, the wheel widens
the view, and `*` or `Esc` gives the camera back exactly as it was, because
nothing about it was touched.

Dragging the sun follows the mouse: down takes it down. The **right** drag turns the camera in every mode, so aiming the light never
costs the view. The **left** drag belongs to the sun, and does from the moment
the page opens: swinging it moves the sun and the camera does not budge, and
the sun is drawn in the sky where it stands so it can be aimed by eye. `L` or
`Esc` hands the left drag back to the camera, `L` again takes it. Turning the
mode on turns **Shadows** on, since it would otherwise do nothing that can be
seen, and the drawn sun goes away with either of them. The mode itself is not
saved.

A finger has no second button, so on a touch screen every drag is the camera's
and the sun is aimed with **Sun across** and **Sun up**.

A run with shadows on reports them, because a second ray per lit pixel is not
the same work as one.

**The test cube** is build step 6 on the Chrome object, off by default. It is a
plain box hanging clear of the slab, and it exists to be checked by eye: put the
relief away, leave the slab flat, and its shadow is one anybody can predict. It
is taken in the untwirled coordinates so the twirl does not drag it, it is
carried by the same distance bound as the slab, and the bounding volume grows to
reach it. Nothing about it is clever, which is the point.

A sun low on the horizon is not the same thing as a shadow. On a relief steep
enough, most of the surface faces away from a sun a few degrees up, and looking
toward that sun shows almost nothing but the sides that do — dark because they
are turned away, with no ray cast at all. **Sun up** is the control that
separates the two.

## 16. MatCap

A matcap is a photograph of a sphere. The surface is coloured by looking that
photograph up by which way it points **on screen** — so whatever lit the sphere
lights the object, key light, reflections, horizon and all, out of one texture
fetch and no lights at all.

| Control | |
| --- | --- |
| **MatCap** | off / on. Off by default, and off is exact: not a texel is read and the frame is the lit one, to the byte. |
| **Sphere** | Which one. Two are built in — **Chrome**, a polished ball on a dark stand, and **Normals**, which is not a photograph but the surface direction written straight into the colour. |
| **Add…** | Load an image from this computer. It is brought down to 512 across, joins the list and is used at once. |
| **Keep** | Hold on to the chosen sphere — in this browser, and in the cloud so it follows the page to another computer. The two built-in ones are already there. |
| **Delete** | Remove the chosen sphere from both. The built-in two stay. |

The same list feeds the **Shadow sphere** picker in §15, so a sphere loaded once
can be used as a material, as a shadow, or as both.

Switched on it **replaces the material outright**: no key light, no sun, no
shadow ray, no rim, no specular. Fog and the volumetric glow still apply — they
are the air between the camera and the object, not the object's surface. A
benchmark run says which sphere was used, and does not report the sun, because
the sun took no part in it.

The lookup is the surface normal in the camera's own axes, with the camera roll
taken back out so the sphere stays locked to the screen rather than leaning with
the horizon. The texture is already in display colours and the frame is tone
mapped and gamma corrected after shading, so the sample is fed through the tone
map's own inverse first and the two cancel: the pixel that comes out is the
pixel that went in.

The two built-in spheres are generated by `tools/matcaps.mjs` rather than pasted
in as blobs — that file is the record of what is in each one.

A loaded sphere lasts for the session unless it is **Kept**, which writes it to
this browser and to one small cloud document of its own — one document per
sphere, because an image will not fit alongside the settings. On opening, the
browser's copies land first and the cloud's are merged in by id, so a sphere
kept on another machine arrives without doubling up one already here. The saved
defaults carry which sphere was chosen, never the image.

## 17. Post

What happens to the frame after the object is drawn.

| Control | Range | Default |
| --- | --- | --- |
| **Noise** | 0-2 | 0.00 |

**Noise** is film grain, laid on last, per pixel and per frame. It used to be on
at a fixed strength and there was no way to turn it off; it is now off unless it
is asked for. The native port has no control for it, so `pc/tools/parity.mjs`
compares the two shaders with the page's grain switched back on.

## 18. Typing a value

Every slider's reading can be clicked and typed into. The slider is a
convenience and not the range: what is typed is taken as it is, even when the
thumb has to sit at one end to show it, so a value past either end of a slider
is reached by typing it. `Enter` commits, `Esc` puts the old reading back.
