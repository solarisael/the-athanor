shader_type canvas_item;
render_mode unshaded;

uniform int phase : hint_range(0, 3) = 0;
uniform vec4 tone_a : source_color = vec4(0.018, 0.020, 0.027, 1.0);
uniform vec4 tone_b : source_color = vec4(0.18, 0.20, 0.25, 1.0);
uniform vec4 accent : source_color = vec4(0.76, 0.64, 0.45, 1.0);
uniform float motion_scale : hint_range(0.0, 1.0) = 1.0;
uniform float chromatic_strength : hint_range(0.0, 1.0) = 0.18;

float hash21(vec2 p) {
	p = fract(p * vec2(123.34, 456.21));
	p += dot(p, p + 45.32);
	return fract(p.x * p.y);
}

float value_noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash21(i), hash21(i + vec2(1.0, 0.0)), f.x),
		mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0, 1.0)), f.x), f.y);
}

void fragment() {
	vec2 uv = UV;
	vec2 p = uv * 2.0 - 1.0;
	float t = TIME * motion_scale;
	float field = 0.0;
	float spark = 0.0;

	if (phase == 0) {
		float soot = value_noise(uv * 8.0 + vec2(0.0, t * 0.08));
		float ash = smoothstep(0.82, 0.98, value_noise(uv * 31.0 - vec2(0.0, t * 0.12)));
		field = soot * 0.62 + ash * 0.18;
	} else if (phase == 1) {
		float pane = abs(sin((uv.x * 3.0 + uv.y * 1.4 + t * 0.05) * 3.14159265));
		float caustic = pow(1.0 - pane, 7.0);
		float frost = value_noise(uv * 13.0 + vec2(t * 0.03, 0.0));
		field = 0.18 + frost * 0.38 + caustic * 0.44;
		spark = caustic * 0.26;
	} else if (phase == 2) {
		float radius = length(p);
		float rings = 0.5 + 0.5 * cos(radius * 28.0 - t * 0.9);
		float dust = smoothstep(0.89, 0.99, value_noise(uv * 38.0 + vec2(t * 0.04, 0.0)));
		field = 0.24 + rings * 0.38 + dust * 0.34;
		spark = dust * 0.30;
	} else {
		float ember = smoothstep(0.92, 0.995, value_noise(uv * 35.0 - vec2(0.0, t * 0.22)));
		float heat = 0.5 + 0.5 * sin((uv.y * 8.0 + value_noise(uv * 5.0) * 2.4 - t * 0.34) * 3.14159265);
		field = 0.22 + heat * 0.42 + ember * 0.32;
		spark = ember * 0.38;
	}

	field = clamp(field, 0.0, 1.0);
	vec3 color = mix(tone_a.rgb, tone_b.rgb, field);
	color += accent.rgb * spark;

	float edge = smoothstep(0.22, 0.0, min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y)));
	float split = (value_noise(uv * 17.0 + t * 0.07) - 0.5) * edge * chromatic_strength;
	color.r += max(split, 0.0) * 0.24;
	color.b += max(-split, 0.0) * 0.22;

	float vignette = 1.0 - smoothstep(0.58, 1.12, length(p));
	color *= mix(0.70, 1.0, vignette);
	COLOR = vec4(color * COLOR.rgb, COLOR.a);
}
