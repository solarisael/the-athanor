shader_type canvas_item;

uniform float pixel_size : hint_range(1.0, 64.0) = 1.0;
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_nearest;

void fragment() {
	vec2 size = vec2(textureSize(SCREEN_TEXTURE, 0));
	vec2 uv = SCREEN_UV;
	vec2 pixel_coord = floor(uv * size / pixel_size) * pixel_size;
	vec2 snapped_uv = pixel_coord / size;
	COLOR = texture(SCREEN_TEXTURE, snapped_uv);
}
