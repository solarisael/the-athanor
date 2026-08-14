@tool
class_name TextAction
extends Button

@export var label_text: String = "Cancel":
	set(value):
		label_text = value
		text = value

func _ready() -> void:
	text = label_text
