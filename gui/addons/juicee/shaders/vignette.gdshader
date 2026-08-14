shader_type canvas_item;

uniform float intensity : hint_range(0.0, 1.0) = 0.0;
uniform float softness : hint_range(0.0, 1.0) = 0.45;
uniform vec4 vignette_color : source_color = vec4(0.0, 0.0, 0.0, 1.0);
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_linear_mipmap;

void fragment() {
	vec2 uv = SCREEN_UV;
	vec4 screen_color = texture(SCREEN_TEXTURE, uv);

	vec2 center = uv - vec2(0.5);
	float dist = length(center);
	float vignette = smoothstep(0.5, 0.5 - softness, dist * (1.0 + intensity));
	vignette = 1.0 - vignette;
	vignette = clamp(vignette * intensity, 0.0, 1.0);

	COLOR = mix(screen_color, vignette_color, vignette);
}
