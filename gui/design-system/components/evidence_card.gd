@tool
extends Control
class_name EvidenceCard

@export var title_text: String = "Evidence":
	set(value):
		title_text = value
		_apply_content()

@export var meta_text: String = "—":
	set(value):
		meta_text = value
		_apply_content()

@export_multiline var excerpt_text: String = "":
	set(value):
		excerpt_text = value
		_apply_content()

@export var source_path_text: String = "—":
	set(value):
		source_path_text = value
		_apply_content()

func _ready() -> void:
	_apply_content()

func _apply_content() -> void:
	if not has_node("Surface/Content/Column"):
		return
	$Surface/Content/Column/Title.text = title_text
	$Surface/Content/Column/Meta.text = meta_text
	$Surface/Content/Column/Excerpt.text = excerpt_text
	$Surface/Content/Column/SourcePath.text = source_path_text
