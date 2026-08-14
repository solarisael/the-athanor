shader_type canvas_item;

uniform float strength : hint_range(0.0, 1.0) = 0.0;
uniform float time_offset : hint_range(0.0, 100.0) = 0.0;
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_nearest;

float hash(float x) {
	return fract(sin(x) * 43758.5453);
}

void fragment() {
	vec2 uv = SCREEN_UV;
	float t = TIME + time_offset;

	// Horizontal slice offset
	float slice = floor(uv.y * 60.0);
	float slice_seed = hash(slice + floor(t * 25.0));
	float slice_offset = (slice_seed - 0.5) * strength * 0.1;

	// Apply only if seed exceeds threshold (selective glitch)
	if (slice_seed > 1.0 - strength) {
		uv.x += slice_offset;
	}

	// Sample with chromatic split scaled by strength
	float split = strength * 0.005;
	float r = texture(SCREEN_TEXTURE, uv + vec2(split, 0.0)).r;
	float g = texture(SCREEN_TEXTURE, uv).g;
	float b = texture(SCREEN_TEXTURE, uv - vec2(split, 0.0)).b;

	COLOR = vec4(r, g, b, 1.0);
}
