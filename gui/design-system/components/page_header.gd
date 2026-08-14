@tool
extends VBoxContainer
class_name PageHeader

@export_multiline var kicker_text: String = "":
	set(value):
		kicker_text = value
		_sync_text()

@export_multiline var title_text: String = "":
	set(value):
		title_text = value
		_sync_text()

@export_multiline var lead_text: String = "":
	set(value):
		lead_text = value
		_sync_text()

func _ready() -> void:
	_sync_text()

func _sync_text() -> void:
	var kicker := get_node_or_null("Kicker") as Label
	var title := get_node_or_null("Title") as Label
	var lead := get_node_or_null("Lead") as Label
	if kicker != null:
		kicker.text = kicker_text
		kicker.visible = not kicker_text.is_empty()
	if title != null:
		title.text = title_text
	if lead != null:
		lead.text = lead_text
		lead.visible = not lead_text.is_empty()
