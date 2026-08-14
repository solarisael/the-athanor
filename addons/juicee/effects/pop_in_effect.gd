## Scale from zero (or a small seed scale) to the node's current size with spring overshoot.
##
## Works on both Node2D and Control nodes.
## Use for: menu pop-in, item spawn appear, notification badge, collectible spawn,
## dialogue bubble, damage indicator, achievement unlock.
##
## Uses Godot's TRANS_SPRING easing which produces a natural overshoot + settle.
@tool
class_name JuiceePopInEffect
extends JuiceeEffect

## Starting scale as a fraction of the original (0 = invisible, 0.5 = half-size start).
@export_range(0.0, 0.99, 0.01) var from_scale: float = 0.0
## Total animation duration including spring settle.
@export_range(0.05, 2.0, 0.05) var duration: float = 0.35

func get_category_color() -> Color: return Color(0.22, 0.78, 0.45)
func get_category_name() -> String: return "Object"

func _apply(context: Node, intensity_mult: float) -> void:
	if not context or not context.is_inside_tree():
		return
	if not (context is Node2D or context is Control):
		push_warning("JuiceePopInEffect: context must be a Node2D or Control")
		return

	var original_scale: Vector2 = context.get("scale")
	var seed_scale := Vector2(from_scale, from_scale) * intensity_mult

	var tween := _track(context.create_tween())
	tween.tween_property(context, "scale", original_scale, duration)\
		.set_trans(Tween.TRANS_SPRING).set_ease(Tween.EASE_OUT)\
		.from(seed_scale)
	await tween.finished
