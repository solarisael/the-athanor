@tool
class_name FieldRow
extends VBoxContainer

@export var label_text: String = "Label":
	set(value):
		label_text = value
		if is_instance_valid(_label):
			_label.text = value
@export var placeholder: String = "":
	set(value):
		placeholder = value
		if is_instance_valid(_field):
			_field.placeholder_text = value
@export var read_only_reason: String = "":
	set(value):
		read_only_reason = value
		_apply_read_only_state()

@onready var _label: Label = %Label
@onready var _field: LineEdit = %Field
@onready var _reason: Label = %ReadOnlyReason

func _ready() -> void:
	_label.text = label_text
	_field.placeholder_text = placeholder
	_apply_read_only_state()

func _apply_read_only_state() -> void:
	if not is_instance_valid(_field):
		return
	var is_read_only := not read_only_reason.is_empty()
	_field.editable = not is_read_only
	_field.tooltip_text = read_only_reason
	_reason.text = read_only_reason
	_reason.visible = is_read_only
