@tool
extends VBoxContainer
class_name ReliquaryPanel

@export var section_title: String = "":
	set(value):
		section_title = value
		_sync_title()

func _ready() -> void:
	_sync_title()

func _sync_title() -> void:
	var header := get_node_or_null("SectionHeader") as Label
	if header != null:
		header.text = section_title.to_upper()
		header.visible = not section_title.is_empty()
