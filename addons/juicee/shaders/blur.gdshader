shader_type canvas_item;

uniform float blur_amount : hint_range(0.0, 8.0) = 0.0;
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_linear_mipmap;

void fragment() {
	vec2 uv = SCREEN_UV;
	vec2 size = vec2(textureSize(SCREEN_TEXTURE, 0));
	vec2 px = blur_amount / size;

	vec4 color = vec4(0.0);
	float total_weight = 0.0;

	// 9-tap gaussian-ish kernel
	for (int x = -1; x <= 1; x++) {
		for (int y = -1; y <= 1; y++) {
			float w = 1.0 / (1.0 + float(x * x + y * y));
			color += texture(SCREEN_TEXTURE, uv + vec2(float(x), float(y)) * px) * w;
			total_weight += w;
		}
	}

	COLOR = color / total_weight;
}
