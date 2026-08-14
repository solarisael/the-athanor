@tool
class_name DisclosureBanner
extends PanelContainer

enum Preset { NO_AUTHORITY, MOCK_NO_BRIDGE, HANDOFF_ORIENTATION, ABSENT_CONTRACT }

const COPY_BY_PRESET := {
	Preset.NO_AUTHORITY: "NO AUTHORITY · THIS PANEL SHOWS ONLY THE SANITIZED ATHANOR HOST RECEIPT",
	Preset.MOCK_NO_BRIDGE: "mock — no bridge; nothing is written.",
	Preset.HANDOFF_ORIENTATION: "Handoff — orientation for the next session. Not a transcript, and not authoritative memory.",
	Preset.ABSENT_CONTRACT: "This surface's Host contract is not yet served. Nothing here is synthesized.",
}

@export var preset: Preset = Preset.NO_AUTHORITY:
	set(value):
		preset = value
		_apply_preset()

@onready var _copy: Label = %Copy

func _ready() -> void:
	_apply_preset()

func _apply_preset() -> void:
	if is_instance_valid(_copy):
		_copy.text = COPY_BY_PRESET[preset]
