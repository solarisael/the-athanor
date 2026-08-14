// Simple 8-direction outline for sprite-style CanvasItems.
// outline_width is in pixels; outline_color tints the border ring.
shader_type canvas_item;

uniform float outline_width : hint_range(0.0, 16.0) = 0.0;
uniform vec4 outline_color : source_color = vec4(1.0, 1.0, 1.0, 1.0);

void fragment() {
	vec4 col = texture(TEXTURE, UV);

	if (outline_width > 0.0 && col.a < 0.5) {
		vec2 px = outline_width / vec2(textureSize(TEXTURE, 0));
		float max_a = 0.0;
		max_a = max(max_a, texture(TEXTURE, UV + vec2( px.x, 0.0)).a);
		max_a = max(max_a, texture(TEXTURE, UV + vec2(-px.x, 0.0)).a);
		max_a = max(max_a, texture(TEXTURE, UV + vec2(0.0,  px.y)).a);
		max_a = max(max_a, texture(TEXTURE, UV + vec2(0.0, -px.y)).a);
		max_a = max(max_a, texture(TEXTURE, UV + vec2( px.x,  px.y)).a);
		max_a = max(max_a, texture(TEXTURE, UV + vec2(-px.x,  px.y)).a);
		max_a = max(max_a, texture(TEXTURE, UV + vec2( px.x, -px.y)).a);
		max_a = max(max_a, texture(TEXTURE, UV + vec2(-px.x, -px.y)).a);
		if (max_a > 0.5) {
			col = outline_color;
		}
	}

	COLOR = col;
}
