@tool
class_name ConsequenceButton
extends Button

enum Consequence { DURABLE, DESTRUCTIVE, SAFE, CANCEL }

@export var consequence: Consequence = Consequence.DURABLE:
	set(value):
		consequence = value
		_apply_consequence()
@export var disabled_reason: String = "":
	set(value):
		disabled_reason = value
		_apply_disabled_state()

@onready var _inner_border: Panel = %InnerBorder
@onready var _reason: Label = %DisabledReason

func _ready() -> void:
	_apply_consequence()
	_apply_disabled_state()

func _apply_consequence() -> void:
	if not is_instance_valid(_inner_border):
		return
	_inner_border.visible = consequence == Consequence.DESTRUCTIVE
	match consequence:
		Consequence.DURABLE:
			add_theme_stylebox_override("normal", preload("res://design-system/components/consequence_button_durable.tres"))
			add_theme_stylebox_override("hover", preload("res://design-system/components/consequence_button_durable.tres"))
			add_theme_stylebox_override("pressed", preload("res://design-system/components/consequence_button_durable.tres"))
			theme_type_variation = &"AthanorBody"
		Consequence.DESTRUCTIVE:
			add_theme_stylebox_override("normal", preload("res://design-system/components/consequence_button_destructive_outer.tres"))
			add_theme_stylebox_override("hover", preload("res://design-system/components/consequence_button_destructive_outer.tres"))
			add_theme_stylebox_override("pressed", preload("res://design-system/components/consequence_button_destructive_outer.tres"))
			theme_type_variation = &"AthanorBody"
		Consequence.SAFE:
			add_theme_stylebox_override("normal", preload("res://design-system/components/consequence_button_safe.tres"))
			add_theme_stylebox_override("hover", preload("res://design-system/components/consequence_button_safe.tres"))
			add_theme_stylebox_override("pressed", preload("res://design-system/components/consequence_button_safe.tres"))
			theme_type_variation = &"AthanorKicker"
		Consequence.CANCEL:
			add_theme_stylebox_override("normal", preload("res://design-system/components/consequence_button_cancel.tres"))
			add_theme_stylebox_override("hover", preload("res://design-system/components/consequence_button_cancel.tres"))
			add_theme_stylebox_override("pressed", preload("res://design-system/components/consequence_button_cancel.tres"))
			theme_type_variation = &"AthanorKicker"

func _apply_disabled_state() -> void:
	disabled = not disabled_reason.is_empty()
	tooltip_text = disabled_reason
	if not is_instance_valid(_reason):
		return
	_reason.text = disabled_reason
	_reason.visible = disabled
	custom_minimum_size.y = 54.0 if disabled else 0.0
