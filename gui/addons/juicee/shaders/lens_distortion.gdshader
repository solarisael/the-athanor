shader_type canvas_item;

uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_linear_mipmap;
/// Positive = barrel distortion (edges bow out). Negative = pincushion (edges bow in).
uniform float strength : hint_range(-1.0, 1.0) = 0.25;
/// How much of the strength to apply this frame (animated by the effect).
uniform float intensity : hint_range(0.0, 1.0) = 1.0;

void fragment() {
	float s = strength * intensity;
	vec2 uv = SCREEN_UV * 2.0 - 1.0;
	float r2 = dot(uv, uv);
	vec2 distorted = uv * (1.0 + s * r2);
	distorted = distorted * 0.5 + 0.5;
	// Clamp to black outside the distorted edges.
	if (distorted.x < 0.0 || distorted.x > 1.0 || distorted.y < 0.0 || distorted.y > 1.0) {
		COLOR = vec4(0.0, 0.0, 0.0, 1.0);
	} else {
		COLOR = texture(SCREEN_TEXTURE, distorted);
	}
}
