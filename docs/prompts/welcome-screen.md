# Welcome screen

The first screen a creator sees: a single `460px` card centred on the window,
with a one-tap **Sign in with OpenParty** and the four OAuth scopes listed before
they leave the app. The plate beneath it carries the brand; the card carries the
task.

## What lands in the centre

The app draws all of this itself, in Figtree, on a `#18181B` card with a white
10% hairline and `14px` corners. None of it may appear in the artwork.

| Order | Native widget |
| --- | --- |
| 1 | Screen title — *Welcome to PartyTime* — plus *Step 1 of 4* and the appearance menu |
| 2 | Lead line — *Publish to a party from this machine.* |
| 3 | Supporting line — *Your browser handles sign-in…* |
| 4 | Primary button — *Sign in with OpenParty*, full width, with an external-link icon |
| 5 | Progress or stage message, while signing in |
| 6 | Four scope rows, each with a small green check |
| 7 | Footer, and a warning line when no system credential store was found |

The plate is therefore authored around a hole: **x 34%–66%, y 10%–90% stays
flat `#09090B`.** The palette, geometry rules and negative-prompt baseline are in
[`README.md`](README.md) and apply to everything below.

Two directions, each with a still and a matching 10-second loop. Direction A is
soft and atmospheric; Direction B is technical and graphic. Pick one per surface
and stay consistent — mixing both across screens is what makes a product look
cobbled together.

## Direction A — Stage wash

A lit surface in a dark room. One wide azure wash raking in from the lower left,
a dim counter-glow top right, one hairline for scale. Atmosphere with no objects.

### Image prompt A

```text
Abstract dark-mode application background art. 16:9, 2560x1440, flat and
orthographic, no perspective, no camera.

SUBJECT — one soft wash of azure stage light raking across a near-black studio
wall, as though a single overhead par light sits just off-frame to the left. The
light is a wide, low-frequency ellipse of colour. It lives entirely in the left
third of the frame and along the bottom edge, and it dissolves completely before
it reaches the middle. It is light on a surface, not an object: no lamp, no
source, nothing with an edge you could point at.

COLOUR — base flat #09090B, even and unmodulated across the whole frame. The
light ramp runs #0084D1 at its densest, through #0069A8, into #74D4FF only as a
thin bright rim on the outermost boundary of the glow — bright cyan used as an
accent, never as a fill. One much dimmer counter-glow of #00598A sits in the top
right corner at roughly 20% of the primary's strength. The brightest pixel in the
image sits inside the lower-left third; overall mean luminance stays close to
#09090B.

SURFACE — an extremely fine, even film grain at 1–2% amplitude across the entire
frame, purely to stop gradient banding. One faint 1px horizontal hairline of
white at 6% opacity runs the full width at 22% from the top, reading as the edge
of a mixing desk or a rack unit in shadow. Nothing else on the surface.

RESERVED CENTRE — the rectangle from 34% to 66% of the width and 10% to 90% of
the height must be flat #09090B: no gradient, no shape, no texture variation, no
highlight. A 508px rounded card will sit there. Keep a soft 6% feather around
that rectangle in which nothing rises more than 8% above the base, and keep the
outer 4% at every edge clear of detail.

MOOD — quiet, expensive, confident. The restraint of a studio monitor photographed
in a dark room; the dark surfaces of a professional creative tool crossed with a
streamer's brand gradient at a fraction of its intensity. The fun is in the
quality of the light, not in the shapes.

FINISH — matte. No vignette frame, no glass, no reflections, no lens flare, no
visible banding, no watermark.
```

### Video prompt A

```text
10.0 second seamless loop, 30 fps, 300 frames, 2560x1440, a single continuous
shot with no cuts.

PLATE — identical to the still: flat #09090B base; one wide azure wash living in
the left third and bottom edge, ramping #0084D1 to #0069A8 with a thin #74D4FF
rim; one dim #00598A counter-glow in the top-right corner at 20% strength; one
1px white hairline at 6% opacity running the full width at 22% from the top; 1–2%
static grain.

MOTION — everything is held still except three things:
1. The primary azure wash drifts slowly to the right and down along a straight
   path, covering 2.5% of the frame width and 1.5% of the frame height over the
   full 10 seconds, easing in and out so it is at rest at 0s and at rest again at
   10s.
2. Its opacity breathes on exactly one sine cycle: 100% at 0s, 118% at 5s, 100%
   at 10s. One peak, one trough, never a second cycle.
3. The counter-glow counter-drifts left by 1% of the frame width, so the two
   light sources separate once and re-meet exactly once.

Everything else is frozen: the hairline does not move, the grain does not crawl,
the arcs and edges of the wash never change shape.

LOOP — frame 300 is visually identical to frame 0. No cut, no fade to black, no
dissolve, no push-in, no zoom, no whip pan, no camera shake. Motion is confined
to the outer thirds: the reserved centre never changes, with no shimmer, no
drifting highlight and no animated noise inside it.

MOOD OF THE MOTION — slow, weighted, ambient. The room breathing, not a
screensaver. At a glance the eye should land on the centre, where the app's card
is, and the motion should register only as something alive at the edge of vision.
```

---

## Direction B — Signal bloom

A broadcast instrument at rest. One lit meter column, concentric transmitter
arcs, a calibration scale. Graphics rather than atmosphere.

### Image prompt B

```text
Abstract dark-mode application background art. 16:9, 2560x1440, perfectly flat
and orthographic, no perspective, no camera.

SUBJECT — a broadcast-signal motif at rest. In the right third of the frame runs
a single vertical column of azure light, 6% of the frame width, from the top edge
to the bottom edge, with a soft falloff on both sides: a lit meter channel. From
that column, five concentric hairline arcs of decreasing opacity — about #74D4FF
at 18%, stepping down to #00598A at 6% — sweep outward across the right half.
They are 1px lines, evenly spaced, each fainter than the last, like the rings of
a transmitter seen edge-on.

In the left third, a quiet counterweight: a flat #0E0E12 field with two 1px
horizontal rules of white at 6% opacity, and three short 24px tick marks in
#00598A spaced evenly near the bottom-left corner, reading as a calibration
scale. Everything else on the frame is flat #09090B.

RESERVED CENTRE — the rectangle from 34% to 66% of the width and 10% to 90% of
the height is flat #09090B, unbroken by any line, arc or glow. A 508px rounded
card will sit there. Keep a 6% feather around it and keep the outer 4% at every
edge clear. The faintest arc may enter the feather and stop; no arc may cross
into the centre.

MOOD — precise, engineered, restrained. An oscilloscope at rest, a broadcast
control room at three in the morning. High-class and technical rather than soft.
The line work is the decoration; there is no illustration, no object and no
metaphor beyond the signal itself.

FINISH — flat vector-abstract. No shadows, no 3D, no bevel, no texture except a
whisper of grain to prevent banding, no watermark.
```

### Video prompt B

```text
10.0 second seamless loop, 30 fps, 300 frames, 2560x1440, a single continuous
shot with no cuts.

PLATE — identical to the still: flat #09090B base; one vertical azure light
column 6% of frame width in the right third, running edge to edge with a soft
falloff; five concentric 1px arcs from that column, #74D4FF at 18% stepping to
#00598A at 6%; a flat #0E0E12 field in the left third with two 1px white rules at
6% and three #00598A tick marks near the bottom-left corner; a whisper of grain.

MOTION — everything is held still except three things:
1. The vertical column travels: from its resting position at 0s it rises 6% of
   the frame height over the first half, then returns to exactly that resting
   position by 10s, on a single ease-in-out cycle.
2. The column's intensity breathes once: 100% at 0s, 125% at 5s, 100% at 10s.
3. The five arcs expand outward from the column and return — one slow pulse. At
   5s each arc sits 1.5% of frame width beyond its resting radius, and at 10s all
   five are back at rest. They never change weight, never flicker, never change
   number.

Everything else is frozen: the rules and tick marks do not move, the grain does
not crawl.

LOOP — frame 300 is visually identical to frame 0. No cut, no fade, no zoom, no
shake, no morphing. Nothing inside the reserved centre changes, including the
arcs as they pulse: their maximum reach must stay outside the 6% feather.

MOOD OF THE MOTION — one slow breath of a machine at idle. Technical and calm,
closer to a fan spinning up than to an equaliser visualisation. Nothing pulses
faster than the eye can follow once.
```

---

## Logo mark

Flat vector, built as a family. One object per app, drawn from identical
construction rules and an identical gradient, so five marks read as five products
of one house — Gmail is an envelope, Docs is a page, and both are unmistakably
Google.

Flat vector is the format that actually ships. A mark has to hold at 16px in a
title bar, at 1024px on a desktop tile, and on a retina panel. Only vector does
all three from one artwork, and gpui-kit resolves `.svg` assets directly, so the
master is an SVG.

### Why the family holds together

Four rules. Every mark obeys all four, and breaking any one of them is how a
suite starts to look assembled rather than designed.

1. **One gradient, byte-identical.** The same three stops at the same angle in
   every app. This is the strongest family signal — stronger than the style,
   stronger than the object.
2. **One construction language.** A 24-unit grid, a 2-unit stroke with round caps
   and joins, a 3-unit corner radius on every rounded rectangle, and nothing
   anywhere smaller than 2 units across.
3. **One object per app, one outline.** A party hat is a triangle. A balloon
   cluster is a scalloped lump. A gong is a circle inside a frame. Two marks in
   the family may never share an outline.
4. **Azure only.** Two or three tones of one hue plus transparency. No second hue
   appears anywhere in the family, ever.

### The house style — paste at the top of every request

```text
FLAT VECTOR APP ICON, geometric and constructed. Not an illustration, not a
photograph, not a 3D render. Everything is built from simple geometric
primitives — circles, arcs, rounded rectangles, triangles, straight strokes —
with flat fills and one shared gradient.

CANVAS — square. The mark occupies the middle 76% of the canvas, with equal
padding on all four sides, so it centres correctly both on a transparent
background and inside a rounded-square app tile.

CONSTRUCTION — drawn on a 24-unit grid. One shape language, one stroke weight of
2 units with rounded caps and rounded joins wherever a stroke is used, one corner
radius of 3 units on every rounded rectangle. No shape smaller than 2 units
across and no stroke thinner than 2 units, because anything finer disappears at
16 pixels. Shapes may overlap, but never with a stroke that crosses another
shape's edge.

LIGHT — implied only, from the top left, through the gradient. There is no drawn
highlight, no cast shadow, no outline stroke and no inner glow. All volume comes
from the gradient itself.

COLOUR — the gradient is identical in every mark: a linear gradient at 135
degrees running #74D4FF at the top left, through #0084D1 at the centre, to #00598A
at the bottom right. Flat fills take #0084D1 or #00598A. Accent details take
#74D4FF at full strength. Transparency replaces white: nothing in the family is
white, and nothing is black except the tile beneath it.

STYLE — modern, confident, engineered. The register of a broadcast control panel
or a well-made developer tool: quiet, geometric, exact. The fun comes from the
choice of object and from how confidently it is drawn, never from decoration.

EXCLUSIONS — no text, no letters, no numbers, no wordmark, no watermark. No
photographic rendering, no 3D, no isometric projection, no gradient mesh, no
soft-focus blur, no glow, no drop shadow, no outline stroke, no noise, no
texture, no grain, no sketch lines.
```

### The gradient, spelled out

| Stop | Position | Hex | Where it is used |
| --- | --- | --- | --- |
| 0% | top left | `#74D4FF` | Accent details; the light side of any highlight edge |
| 50% | centre | `#0084D1` | The main mass of every mark |
| 100% | bottom right | `#00598A` | The shadow side; secondary shapes |

Linear, 135°, in the mark's own 24×24 coordinate space, so it moves correctly
with the artwork. A second mark using a different angle or a different stop is not
part of this family.

### The app tile

Operating systems want a tile, not a floating mark — a transparent logo on an
indistinguishable desktop is invisible. Ship both.

```text
APP TILE — 1024x1024 rounded square, corner radius 22% of the edge length. The
tile is filled with a flat #09090B carrying an almost imperceptible gradient to
#0E0E12 at the bottom right, so the tile reads as a surface rather than as a hole.
The mark is centred inside the tile, occupying 62% of its area. No shadow around
the tile, no border, no inner highlight, no gloss.
```

### PartyTime — the party hat

The console is the moment the party happens. A triangle, a pompom, and nothing
else — the simplest silhouette in the family and the most legible at 16px.

```text
SUBJECT — a party hat drawn as a single rounded triangle, cone pointing up, all
three corners rounded. The cone is filled with the family gradient. A small solid
circle, 3 units across, sits centred exactly on the apex as a pompom, filled
#74D4FF. Below 32 pixels, the two horizontal bands described next are omitted
entirely: at small sizes the mark is the triangle and the dot, nothing else.

AT 32 PIXELS AND ABOVE — two horizontal bands cut clean across the cone as flat
#74D4FF stripes, one across the upper third and one across the lower third,
leaving the middle third as the gradient. The bands are 2 units tall, run edge to
edge across the cone, and are the same width as the cone at that height.

The outline is a triangle: wide flat base, narrow point. No strap, no shadow, no
highlight, nothing below the base line.
```

### OpenParty — the gathering

The platform is the party itself: the room, everyone in it, more than one object.

```text
SUBJECT — three balloons of different sizes drawn as plain circles, grouped so
they touch and slightly overlap into a single silhouette: the largest at the
centre-left, one medium up and to the right, one small filling the gap between
them. Below the group, a short bunting swag drawn as one continuous zigzag
stroke with three downward points, 2 units thick with round caps, running the
full width of the balloon group. At the base, behind the swag, one small rounded
triangle leaning two degrees left — a party hat standing at the back of the
party.

The three circles take the gradient; the swag and the small triangle take flat
#00598A and #74D4FF respectively. Below 32 pixels, drop the swag and the hat:
the mark is three circles.

The outline is a scalloped lump along the top, a zigzag along the bottom. Not a
triangle, and nothing else in the family may have a scalloped top edge.
```

### Three more slots, same grammar

The family scales because each new app picks one object with one unmistakable
outline. Swap these freely; keep the house style byte-identical.

**PartyMod — the gong.** `SUBJECT — a solid circle 15 units across filled with
the gradient, surrounded by a 2-unit open ring in flat #00598A at 22 units across
with a 40-degree break at the top where the gong hangs. Two short 2-unit vertical
strokes drop from the ring's shoulders to a horizontal base line, forming a stand.
The silhouette is a circle inside an open frame with two legs.`

**PartyChat — the speech balloon.** `SUBJECT — a rounded rectangle 17 by 13 units
with a 4-unit corner radius, filled with the gradient, with a solid triangular
tail 5 units wide dropping from the middle of its bottom edge and pointing down
and slightly right, filled flat #74D4FF. The tail overlaps the rectangle by one
unit so no seam shows. The silhouette is a rounded rectangle with exactly one
sharp point.`

**PartyMusic — the burst.** `SUBJECT — a solid circle 8 units across filled with
the gradient at the centre, with eight tapered triangular rays radiating outward
at 45-degree intervals, each 5 units long, 3 units wide at the base and coming
to a point, filled flat #00598A. The rays stop 2 units clear of the circle so
the centre never closes up. The silhouette is a spiky circle, and nothing else in
the family may have radial points.`

### The party vocabulary

Two columns, and the split decides what can *be* a mark versus what is only
dressing. Anything in the right-hand column is a scene, not a symbol.

| Eligible for a mark | Scene dressing only |
| --- | --- |
| party hat, pompom, party popper | garland, tablecloth, curtains |
| balloon, balloon arch | fairy lights, string lights, marquee lights |
| bunting swag, pennant flags, garland | punch bowl, tiered cake, cupcakes |
| gong, brass bell, drum | candle, candelabra, champagne tower |
| confetti, streamers, ribbon | pinata, bauble, tinsel, tiara |
| champagne coupe, toast | crowd, hands, toast, dance floor |

### Negative prompt, the marks

```text
photograph, photorealistic, realistic texture, 3D render, CGI, clay render,
isometric, perspective, soft-focus blur, bokeh, drop shadow, outer glow, inner
glow, bevel, emboss, outline stroke, gradient mesh, mesh gradient, noise,
texture, grain, sketch, pencil line, hand-drawn wobble, uneven stroke weight,
off-grid geometry, text, letters, wordmark, numbers, signature, watermark,
white fill, black fill, red, pink, magenta, purple, violet, lime, green, yellow,
orange, gold, rainbow, multicolor, more than three shapes at small size,
details below 2 units, mascot, cartoon face, confetti scattered across the
composition
```

---

## Lockups and proofs

The master is an SVG; everything else is an export of it. Nothing is redrawn per
size.

| Deliverable | Spec |
| --- | --- |
| Master | `mark-<app>.svg`, 24×24 viewBox, on a transparent background |
| UI exports | PNG at 512, 256, 128, 64, 32 and 16, exported from the master, no resampling |
| Two-tone export | The gradient replaced by a flat `#0084D1` fill, for contexts where the gradient is lost |
| App tile | `tile-<app>@1024.png` per the tile spec above |
| Lockup | Mark, then a 4-unit gap, then the wordmark set live in Figtree SemiBold, sentence case, 0.01em tracking, optically centred against the mark's mass |

**Mark acceptance checks.** Thumbnail it at 16px: identifiable without being told
what it is. Fill the whole silhouette black and check the outline is still
distinct from every other mark in the family. Export it flat and confirm the
object still reads. Set PartyTime and OpenParty side by side at 24px: same
gradient, unmistakably different objects. Place the 32px lockup against the
wordmark: one object, not an icon with a caption. And check the whole family on
one sheet — if one mark is carrying noticeably more detail than the others,
simplify that one rather than enriching the rest.

**Where it goes.** The welcome header at 24px, above and left-aligned with the
*Welcome to PartyTime* title. The producer view takes the 16px export in the
title bar. Nowhere else until the light appearance exists.

---

## Delivery

| Asset | File | Spec |
| --- | --- | --- |
| Still, direction A | `welcome-a@2560x1440.png` | 16:9, sRGB, lossless |
| Still, direction B | `welcome-b@2560x1440.png` | 16:9, sRGB, lossless |
| Loop, direction A | `welcome-a@10s.mp4` | H.264, 30 fps, 300 frames, silent |
| Loop, direction B | `welcome-b@10s.mp4` | H.264, 30 fps, 300 frames, silent |
| Mark, PartyTime | `mark-partytime.svg` + `.png` at 6 sizes + `tile-partytime@1024.png` | SVG master on a 24-unit grid |
| Mark, OpenParty | `mark-openparty.svg` + `.png` at 6 sizes + `tile-openparty@1024.png` | SVG master on a 24-unit grid |
| Marks, sibling slots | `mark-<app>.svg` + `.png` at 6 sizes | Same three files per slot |
| Lockup | `lockup-partytime@4x.png` | Mark, 4-unit gap, wordmark in Figtree SemiBold |

Author plates at 2× the nominal window size and let the app crop, so a plate
survives a resized window. Keep every source file; ship compressed derivatives.

## Checking a plate before it ships

1. **Centre test.** Crop the rectangle x 34%–66%, y 10%–90% and stretch it. It
   must read as one flat field with no structure at any zoom.
2. **Contrast test.** Drop a `#18181B` rounded card, `14px` corners and a white
   10% hairline, at the exact card size, centred. The card must be the brightest
   thing on screen after the artwork's own highlight.
3. **Hierarchy test.** Desaturate the plate. *Sign in with OpenParty* must still
   be the first thing the eye finds.
4. **Loop test.** Play the video on repeat for two full cycles. No frame may
   pulse, flicker or jump at the seam, and nothing inside the centre may change.
5. **Reveal test.** Composite the card and its text at 100% and at 60% scale. If
   any part of the card sits on a highlight, feather the plate further.

Once a direction is chosen, the same plate language carries to the splash,
onboarding and producer screens — changing the safe rectangle per screen, never
the palette, the grain or the light direction. The same goes for the marks: one
house style, one gradient, one outline per app.