@tool
extends PanelContainer
class_name RitualSurface

enum Variant { MANTLE, VESSEL, AETHER, ORNAMENT }

@export var variant: Variant = Variant.MANTLE:
	set(value):
		variant = value
		_apply_variant()

var _styles: Array[StyleBoxFlat] = []

func _ready() -> void:
	_apply_variant()

func _apply_variant() -> void:
	if _styles.is_empty():
		_styles = [
			_surface_style(Color(0.0189863, 0.01942671, 0.02301076, 1), 1, Color(0.76193195, 0.64497094, 0.44939537, 1), 6),
			_surface_style(Color(0.03446452, 0.03512804, 0.04040454, 1), 1, Color(0.76193195, 0.64497094, 0.44939537, 1), 10),
			_surface_style(Color(0.05868481, 0.05967549, 0.06723587, 1), 1, Color(0.83797062, 0.72687352, 0.54300893, 1), 14),
			_surface_style(Color(0.00149408, 0.00156714, 0.00227788, 1), 2, Color(0.76193195, 0.64497094, 0.44939537, 1), 3),
		]
	add_theme_stylebox_override(&"panel", _styles[variant])

func _surface_style(background: Color, border_width: int, border: Color, radius: int) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = background
	style.border_width_left = border_width
	style.border_width_top = border_width
	style.border_width_right = border_width
	style.border_width_bottom = border_width
	style.border_color = border
	style.corner_radius_top_left = radius
	style.corner_radius_top_right = radius
	style.corner_radius_bottom_right = radius
	style.corner_radius_bottom_left = radius
	style.content_margin_left = 18.0
	style.content_margin_top = 18.0
	style.content_margin_right = 18.0
	style.content_margin_bottom = 18.0
	return style
