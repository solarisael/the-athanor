shader_type canvas_item;

uniform float saturation : hint_range(0.0, 3.0) = 1.0;
uniform float contrast   : hint_range(0.0, 3.0) = 1.0;
uniform float brightness : hint_range(-1.0, 1.0) = 0.0;
uniform vec3  tint       : source_color = vec3(1.0);
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_linear_mipmap;

void fragment() {
	vec3 col = texture(SCREEN_TEXTURE, SCREEN_UV).rgb;

	// Brightness
	col += brightness;

	// Contrast around 0.5 midpoint
	col = (col - 0.5) * contrast + 0.5;

	// Saturation (lerp from luminance grayscale)
	float lum = dot(col, vec3(0.299, 0.587, 0.114));
	col = mix(vec3(lum), col, saturation);

	// Tint
	col *= tint;

	COLOR = vec4(clamp(col, vec3(0.0), vec3(1.0)), 1.0);
}
