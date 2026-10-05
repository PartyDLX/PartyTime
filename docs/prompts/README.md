# Art prompts — PartyTime / OpenParty Studio

Generated artwork for the console: welcome screen first, then the remaining screens.
Every asset is described twice, once as a still and once as a seamless 10-second
video loop.

- [`welcome-screen.md`](welcome-screen.md) — the sign-in screen background in two
  directions, plus the flat-vector logo mark family.

## How these prompts are used

The app draws its own text, buttons and lists as native widgets. The artwork is
the *plate* the widgets sit on, never a picture of the screen. Every prompt here
therefore does three things:

1. **Describes the art** — light, gradient, shape, texture.
2. **Reserves the centre** — a quiet rectangle the native card occupies.
3. **Forbids text** — no words, no letters, no fake UI are baked into a raster.
   Type is drawn by the app in the bundled face, so it stays selectable,
   localizable and correctly hinted.

That split follows the project design guides: decoration supports the content
rather than competing with it, the base window surface stays flat, and text
stays out of raster assets.

## Brand — dark appearance only

The palette is the OpenParty `.dark` block transcribed in
`crates/studio-console/src/theme.rs` into `oklch()`. The hex values below are
those tokens converted to sRGB so an image model has something concrete to aim
at. They are not new colors.

| Role | Hex | Notes |
| --- | --- | --- |
| `background` | `#09090B` | The flat window surface. The base of every plate. |
| `card` | `#18181B` | The sign-in card. Drawn by the app, never in the art. |
| `secondary` / `muted` | `#27272A` | Quiet fills, mixer surfaces. |
| `foreground` | `#FAFAFA` | Body text. |
| `muted-foreground` | `#9F9FA9` | Secondary text. |
| `primary` | `#00598A` | The primary action, *Sign in with OpenParty*. |
| `primary-foreground` | `#F0F9FF` | Label on that action. |
| `ring` | `#71717B` | Focus ring. |
| `border` | white @ 10% | ≈ `#222223` composited on `background`. A hairline. |
| `input` | white @ 15% | ≈ `#303034` composited on `background`. |
| `destructive` | `#FF6467` | Danger only. Not decoration. |
| `chart-1` … `chart-5` | `#74D4FF` `#00A6F4` `#0084D1` `#0069A8` `#00598A` | Reserved for live bitrate and frame-rate graphs. |

The chart ramp is the brand ramp: **azure, running from a bright `#74D4FF` to a
deep `#00598A`, around hue 230°–241°.** Artwork draws its light from that ramp.
Where a prompt needs a step between two stops — a hotter core, a lift above
`chart-2` — it names it as *derived* and gives the value. Those derived values
are artwork-only; they are not theme tokens and must never reach `theme.rs`.

Two rules keep the brand honest in a raster:

- **Blue stays blue.** Hue stays in 225°–245°. Violet, magenta, sunset orange
  and green-tinted teal are not brand colors and are in the negative prompt of
  every prompt here.
- **Dark means dark.** The mean luminance of a plate sits near `#09090B`. The
  artwork is a lit surface in a dark room, not a dark image on a bright one.

## Type

**Figtree** (variable weight 300–900), bundled. It is used in the artwork only
if a prompt asks for a lockup; otherwise no type at all. Any wordmark the app
shows is drawn as live text.

## Geometry every plate must respect

The welcome card is `460px` wide with `24px` padding on all sides, so its outer
box is `508px`. On a `1280×720` window that is roughly the middle **40%** of the
width. A plate is authored at **2560×1440** and cropped, so it must survive
being letterboxed.

| Zone | Share of canvas | What belongs there |
| --- | --- | --- |
| Quiet centre | x 34%–66%, y 10%–90% | Nothing. Flat `background`, nothing brighter than `#101014`. |
| Feather | 6% around the quiet centre | No element brighter than 8% over the base. Soft light only. |
| Edges | outer 4% | Clear. Window corners are rounded and the app may cast a shadow. |
| Field | everything else | All of the art. |

A plate that puts a focal point inside the quiet centre is rejected, however
beautiful it is: the card lands on top of it and the screen loses its one
reading order.

## Writing a new asset

Copy the structure of `welcome-screen.md` — *Direction* → *Image prompt* →
*Video prompt* → *Negative* → *Delivery*. Then:

- Keep dark appearance only until a light version is actually asked for.
- Keep the quiet centre. Name the widget that lands in it.
- Keep text out. If the screen needs a headline, it is a native widget.
- Keep light sources at or below the periphery; the brightest pixel in the plate
  should never be near the middle.