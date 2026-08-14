@tool
extends PanelContainer
class_name ReceiptCard

@export var title_text: String = "Latest Paper Boat":
	set(value):
		title_text = value
		_apply_content()

@export var timestamp_text: String = "— · ROOM —":
	set(value):
		timestamp_text = value
		_apply_content()

@export var delivered_text: String = "PENDING · waiting for a verified receipt":
	set(value):
		delivered_text = value
		_apply_content()

@export var record_text: String = "—":
	set(value):
		record_text = value
		_apply_content()

@export var event_text: String = "—":
	set(value):
		event_text = value
		_apply_content()

@export var sequence_text: String = "—":
	set(value):
		sequence_text = value
		_apply_content()

@export var sha_text: String = "—":
	set(value):
		sha_text = value
		_apply_content()

func _ready() -> void:
	_apply_content()

func _apply_content() -> void:
	if not has_node("Column/Header"):
		return
	$Column/Header/Title.text = title_text
	$Column/Header/Timestamp.text = timestamp_text
	$Column/Delivered/Value.text = delivered_text
	$Column/Record/Value.text = record_text
	$Column/Event/Value.text = event_text
	$Column/Sequence/Value.text = sequence_text
	$Column/Sha/Value.text = sha_text
