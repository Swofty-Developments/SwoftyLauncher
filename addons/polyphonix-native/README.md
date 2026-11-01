# PolyphoniX Native

The PolyphoniX Native look from Assetto Corsa, by Mark Wazowski, as a Swofty Launcher shader: brighter midtones, a touch more colour, a 6000 K white balance, and a soft glare with a six-point star on bright lights. It runs on the 3D picture before the HUD, so the HUD and menus keep their own colours. Needs Swofty Launcher 1.7.12 or later.

## Playing it

Turn the addon on and press **Play**. In game:

- **B** turns the look on and off, with a `SHADERS ON` / `SHADERS OFF` toast. **On/off key** picks another key or **None**.
- Every slider changes the picture straight away under **ADDON SETTINGS**.

| Setting | Default | Does |
|---|---|---|
| Look strength | 100 | Mixes the grade from the game's picture at 0 to the full look at 100 |
| Brightness | 125 | Lifts the midtones; white stays white |
| Saturation | 110 | Colour, from grey at 0 to the game's own at 100 |
| White balance (K) | 6000 | Colour temperature; 6500 is neutral |
| Contrast | 100 | Contrast around mid grey |
| Bloom | 35 | The soft glow around bright lights and sky |
| Star glare | 30 | The six-point star on the brightest highlights |
| Glare threshold | 82 | How bright a pixel has to be to glow |

## How it works

Eleven post-process passes (see [Post-process passes](../../docs/reference/addons/postfx.md)):

- `fx/bloom.hlsl` `prefilter` keeps the bright parts in a half-size texture, with a soft knee and a Karis average against single bright pixels.
- `down` halves it four more times to 1/32 with a 13-tap filter, and `up` adds each level back on the way up with a 3×3 tent.
- `star` draws six streaks at 3° tilt from the quarter-size level.
- `fx/look.hlsl` adds the glow and the star to the picture in linear light, then grades it.

The options bind `native`, so the core reads them live, and `postfx_toggle` names the key.

## Credit

The look and its default values are Mark Wazowski's PolyphoniX Native preset for Assetto Corsa's Custom Shaders Patch, rebuilt from scratch as HLSL compute passes. MIT for the shaders here.
