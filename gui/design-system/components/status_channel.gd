@tool
extends HBoxContainer
class_name StatusChannel

enum StateKind { OK, PENDING, DEGRADED, REFUSED, ABSENT }

@export var channel_name: String = "CHANNEL":
	set(value):
		channel_name = value
		_apply_content()

@export var state_text: String = "—":
	set(value):
		state_text = value
		_apply_content()

@export var state_kind: StateKind = StateKind.ABSENT:
	set(value):
		state_kind = value
		_apply_content()

func _ready() -> void:
	_apply_content()

func _apply_content() -> void:
	if not has_node("Name"):
		return
	$Name.text = channel_name
	$State.text = state_text
	$Mark/Symbol.text = _glyph_for(state_kind)

func _glyph_for(kind: StateKind) -> String:
	match kind:
		StateKind.OK:
			return "■"
		StateKind.PENDING:
			return "□"
		StateKind.DEGRADED:
			return "◧"
		StateKind.REFUSED:
			return "✖"
		_:
			return "—"
