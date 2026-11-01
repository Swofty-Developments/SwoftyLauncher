#include "swofty/postfx.hlsli"

// The PolyphoniX look on the picture: the glare added in linear light, then a 6000 K white balance,
// a brightness lift that keeps white, contrast around mid grey and saturation.

float3 kelvin_rgb(float kelvin)
{
    float t = kelvin / 100;
    float r = t <= 66 ? 255 : 329.698727446 * pow(t - 60, -0.1332047592);
    float g = t <= 66 ? 99.4708025861 * log(t) - 161.1195681661 : 288.1221695283 * pow(t - 60, -0.0755148492);
    float b = t >= 66 ? 255 : (t <= 19 ? 0 : 138.5177312231 * log(t - 10) - 305.0447927307);
    return srgb_to_linear(saturate(float3(r, g, b) / 255));
}

float3 white_balance(float3 rgb, float kelvin)
{
    float3 gain = kelvin_rgb(kelvin) / kelvin_rgb(6500);
    gain /= luminance(gain);
    return rgb * gain;
}

float3 lift(float3 rgb, float brightness)
{
    float3 c = min(rgb, 1);
    return rgb + c * brightness / (1 + (brightness - 1) * c) - c;
}

float3 contrast(float3 rgb, float amount)
{
    float3 mid = 0.18;
    return mid * pow(max(rgb, 0) / mid, amount);
}

float4 effect(float2 uv, uint2 pixel)
{
    float4 c = scene_at(int2(pixel));
    float3 rgb = to_linear(c.rgb);
    float strength = option(0) / 100, bloom = option(5) / 100, star = option(6) / 100;
    rgb += input(0, uv).rgb * (bloom * 0.35) + input(1, uv).rgb * (star * 0.6);
    float3 base = rgb;
    rgb = white_balance(rgb, option(3));
    rgb = lift(rgb, option(1) / 100);
    rgb = contrast(rgb, option(4) / 100);
    rgb = lerp(luminance(rgb).xxx, rgb, option(2) / 100);
    rgb = lerp(base, max(rgb, 0), strength);
    return float4(from_linear(rgb), c.a);
}
